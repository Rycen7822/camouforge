from __future__ import annotations

import asyncio
import os
import sys
import threading
import time
import traceback
from pathlib import Path
from typing import Any, Dict, Optional

from browser_patches import (
    _ensure_chrome_css_patches,
    _ensure_macos_emoji_font,
    _ensure_macos_emoji_whitelist,
    _ensure_session_history_pref,
    _shortcut_extension_dir,
    _shortcut_page,
)
from downloads import (
    _attach_close_watcher,
    _attach_download_handler,
    _resolve_downloads_dir,
)
from profile_translate import translate_profile
from sdk_bridge import _relocate_sdk_cache
from win_process import _pid_alive

if os.name == "nt":
    from win_process import _find_browser_pid, _tree_has_visible_window


class Instance:
    def __init__(self, profile: Dict[str, Any], headless: bool, user_data_dir: Optional[str]):
        self.profile_id = profile["id"]
        self.profile_name = profile.get("name", profile["id"])
        self.headless = headless
        self.user_data_dir = user_data_dir
        self.pid: Optional[int] = None  # Playwright 驱动（node）pid
        self.browser_pid: Optional[int] = None  # 真实浏览器主进程 pid（驱动的直接子进程）
        self.started_at = _now()
        self.ready = threading.Event()
        self.response_sent = threading.Event()  # launch 响应已 flush（Manager.launch_done 置位）
        self.stop_requested = False
        self.lock = threading.Lock()
        self.alive = True
        self.thread: Optional[threading.Thread] = None
        self.exit_reason: Optional[str] = None
        self.loop: Optional[asyncio.AbstractEventLoop] = None
        self.stop_event: Optional[asyncio.Event] = None
        self.download_tasks: set[asyncio.Task[Any]] = set()
        self.download_slots: Optional[asyncio.Semaphore] = None
        self.download_name_lock: Optional[asyncio.Lock] = None
        self.download_targets: set[Path] = set()
        self.accept_downloads = True

    def wake(self) -> None:
        loop = self.loop
        event = self.stop_event
        if loop is None or event is None:
            return
        try:
            loop.call_soon_threadsafe(event.set)
        except RuntimeError:
            pass  # 事件循环已关闭，无需再唤醒

    def bind_loop(self) -> asyncio.Event:
        with self.lock:
            self.loop = asyncio.get_running_loop()
            event = self.stop_event = asyncio.Event()
            if self.stop_requested:
                event.set()
            return event

    def request_stop(self) -> None:
        with self.lock:
            self.stop_requested = True
        self.wake()

    def notify_exit(self, reason: str) -> None:
        with self.lock:
            if not self.stop_requested and self.exit_reason is None:
                self.exit_reason = reason
        self.wake()


def _now() -> int:
    return int(time.time())


class Manager:
    def __init__(self, emit):
        self.emit = emit
        self.instances: Dict[str, Instance] = {}
        self.big_lock = threading.Lock()
        self.user_data_root = Path(os.environ.get("CAMOFORGE_DATA_DIR", Path.home() / ".camoforge")) / "user_data"
        self.user_data_root.mkdir(parents=True, exist_ok=True)

    def launch(self, profile: Dict[str, Any]) -> Dict[str, Any]:
        # launch 不经 SDK.load()，重定向必须在此触发（见 _relocate_sdk_cache）
        _relocate_sdk_cache()
        pid = profile["id"]
        with self.big_lock:
            if pid in self.instances:
                raise RuntimeError(f"profile {pid} is already running")
            kw = translate_profile(profile, self.user_data_root)
            _ensure_macos_emoji_whitelist()
            _ensure_macos_emoji_font(kw.get("executable_path"))
            _ensure_chrome_css_patches(kw.get("executable_path"))
            _ensure_session_history_pref(kw.get("executable_path"))
            inst = Instance(profile, bool(kw.get("headless")), kw.get("user_data_dir"))
            self.instances[pid] = inst
        downloads_dir = _resolve_downloads_dir(profile)
        # 兜底：部分站点的下载（window.open / File System Access 等）不走 Juggler 的
        # download 事件，而是落回 Firefox 原生下载管理器。把原生下载目录也配置成
        # downloads_dir，保证这类下载同样落到用户指定位置（setdefault 不覆盖用户显式 pref）。
        try:
            downloads_dir.mkdir(parents=True, exist_ok=True)
            prefs = kw.setdefault("firefox_user_prefs", {})
            prefs.setdefault("browser.download.useDownloadDir", True)
            prefs.setdefault("browser.download.folderList", 2)
            prefs.setdefault("browser.download.dir", str(downloads_dir))
            prefs.setdefault("browser.download.always_ask_before_handling_new_types", False)
            prefs.setdefault(
                "browser.helperApps.neverAsk.saveToDisk",
                "application/octet-stream,application/zip,application/pdf,"
                "application/x-apple-diskimage,application/json,text/markdown,"
                "text/plain,application/gzip,application/x-msdownload",
            )
            print(f"[camoforge] 下载目录: {downloads_dir}", file=sys.stderr)
        except Exception:
            pass
        shortcut_page = _shortcut_page(profile)
        # 新标签页覆盖：生成扩展目录并追加进 addons，使每个新标签页（Ctrl+T）
        # 也显示快捷方式网格（与 Chrome 行为一致）。camoufox 会校验目录含 manifest.json。
        ext_dir = _shortcut_extension_dir(profile)
        if shortcut_page or ext_dir:
            prefs = kw.setdefault("firefox_user_prefs", {})
            if shortcut_page:
                prefs["browser.startup.homepage"] = shortcut_page
                prefs["browser.startup.page"] = 1
            if ext_dir:
                # camoufox 默认 browser.newtabpage.enabled=false（新标签页空白）。
                # 覆盖 newtab 的扩展只在 about:newtab 生效，故需重新启用新标签页。
                prefs["browser.newtabpage.enabled"] = True
                addons = list(kw.get("addons") or [])
                if ext_dir not in addons:
                    addons.append(ext_dir)
                kw["addons"] = addons

        async def run_async(inst: Instance) -> None:
            cm = None
            launched = False
            stop_event = inst.bind_loop()
            inst.download_slots = asyncio.Semaphore(2)
            inst.download_name_lock = asyncio.Lock()
            try:
                from camoufox.async_api import AsyncCamoufox

                cm = AsyncCamoufox(**kw)
                browser = await cm.__aenter__()
                launched = True
                # 持久化模式返回 BrowserContext；非持久化模式必须显式创建允许下载的 context。
                persistent = bool(kw.get("persistent_context"))
                page = None
                try:
                    if persistent:
                        pages = getattr(browser, "pages", None)
                        page = pages[0] if pages else await browser.new_page()
                        _attach_download_handler(browser, inst, downloads_dir)
                        _attach_close_watcher(browser, inst)
                    else:
                        context = await browser.new_context(accept_downloads=True)
                        _attach_download_handler(context, inst, downloads_dir)
                        _attach_close_watcher(context, inst)
                        _attach_close_watcher(browser, inst)
                        page = await context.new_page()
                except Exception as e:
                    print(f"[camoforge] 初始页/下载处理器失败: {e}", file=sys.stderr)
                try:
                    inst.pid = browser._impl_obj._connection._transport._proc.pid  # type: ignore[attr-defined]
                except Exception:
                    inst.pid = None
                # inst.pid 是 node 驱动；真实浏览器主进程是其直接子进程。
                # 关窗/崩溃检测以主进程为准（驱动在浏览器死后仍可能存活）。
                if os.name == "nt" and inst.pid:
                    exe_name = (
                        Path(kw["executable_path"]).name
                        if kw.get("executable_path")
                        else "camoufox.exe"
                    )
                    for _ in range(20):
                        inst.browser_pid = _find_browser_pid(inst.pid, exe_name)
                        if inst.browser_pid:
                            break
                        await asyncio.sleep(0.25)
                if page is not None and shortcut_page:
                    try:
                        await page.goto(
                            shortcut_page,
                            wait_until="domcontentloaded",
                            timeout=15_000,
                        )
                    except Exception:
                        pass
                inst.ready.set()
                window_seen = False
                windowless_since: Optional[float] = None
                # Camoufox 关窗后可能常驻且不发关闭事件，需低频检查进程与窗口。
                while not stop_event.is_set():
                    try:
                        await asyncio.wait_for(stop_event.wait(), timeout=2.0)
                        continue
                    except asyncio.TimeoutError:
                        pass
                    now = time.monotonic()
                    watch_pid = inst.browser_pid or inst.pid
                    if watch_pid is not None and not await asyncio.to_thread(
                        _pid_alive, watch_pid
                    ):
                        inst.notify_exit("浏览器已退出（窗口被关闭或进程结束）")
                        break
                    if (
                        os.name == "nt"
                        and not inst.headless
                        and inst.browser_pid is not None
                    ):
                        visible = await asyncio.to_thread(
                            _tree_has_visible_window, inst.browser_pid
                        )
                        if visible:
                            window_seen = True
                            windowless_since = None
                        elif window_seen:
                            windowless_since = windowless_since or now
                            if now - windowless_since >= 1.0:
                                inst.notify_exit("浏览器窗口已关闭")
                                break
            except Exception as e:
                reason = f"{type(e).__name__}: {e}"
                with inst.lock:
                    inst.exit_reason = reason
                    inst.alive = False
                traceback.print_exc(file=sys.stderr)
                inst.ready.set()
            finally:
                inst.accept_downloads = False
                tasks = list(inst.download_tasks)
                for task in tasks:
                    task.cancel()
                if tasks:
                    await asyncio.gather(*tasks, return_exceptions=True)
                if cm is not None:
                    try:
                        await cm.__aexit__(None, None, None)
                    except Exception:
                        pass
                with inst.lock:
                    inst.alive = False
                with self.big_lock:
                    self.instances.pop(inst.profile_id, None)
                if launched:
                    # 保证退出事件晚于 launch 响应，避免 Rust 端先 remove 后 insert。
                    inst.response_sent.wait(timeout=2.0)
                    self.emit("instance_exited", {"profile_id": inst.profile_id,
                                                  "reason": inst.exit_reason or "stopped",
                                                  "started": True})

        def run(inst: Instance) -> None:
            asyncio.run(run_async(inst))

        inst.thread = threading.Thread(
            target=run,
            args=(inst,),
            daemon=True,
            name=f"camoforge-{pid[:8]}",
        )
        inst.thread.start()

        # 等待启动完成或失败（最长 180s：首次可能要解压浏览器）
        if not inst.ready.wait(timeout=180):
            with inst.lock:
                inst.exit_reason = "launch timeout (180s)"
                inst.alive = False
            inst.request_stop()
            raise RuntimeError("launch timeout after 180s")
        with inst.lock:
            if not inst.alive and inst.exit_reason:
                raise RuntimeError(inst.exit_reason)
        return {
            "profile_id": pid,
            "pid": inst.pid,
            "headless": inst.headless,
            "user_data_dir": inst.user_data_dir,
        }

    def launch_done(self, profile_id: str) -> None:
        """launch 响应已 flush 到 stdout：放行该实例的 instance_exited 事件。

        由主派发线程在 send(launch 响应) 之后调用，保证事件晚于响应到达
        Rust 端（配合实例线程 finally 里的 response_sent.wait）。
        """
        with self.big_lock:
            inst = self.instances.get(profile_id)
        if inst is not None:
            inst.response_sent.set()

    def stop(self, profile_id: str) -> Dict[str, Any]:
        with self.big_lock:
            inst = self.instances.get(profile_id)
        if not inst:
            return {"profile_id": profile_id, "stopped": False, "reason": "not running"}
        inst.request_stop()
        inst.thread.join(timeout=30)
        if inst.thread.is_alive():
            # 浏览器未能及时关闭（卡死 / 下载占用等）：如实上报，UI 才能恢复运行态。
            return {"profile_id": profile_id, "stopped": False,
                    "reason": "stop timeout (30s)，浏览器可能仍在运行"}
        return {"profile_id": profile_id, "stopped": True}

    def stop_all(self) -> Dict[str, Any]:
        with self.big_lock:
            ids = list(self.instances.keys())
        stopped: list = []
        failed: Dict[str, str] = {}
        for pid in ids:
            try:
                r = self.stop(pid)
                if r.get("stopped"):
                    stopped.append(pid)
                else:
                    failed[pid] = r.get("reason") or "unknown"
            except Exception as e:
                failed[pid] = f"{type(e).__name__}: {e}"
        return {"stopped": stopped, "failed": failed}

    def list(self) -> Dict[str, Any]:
        with self.big_lock:
            items = [
                {
                    "profile_id": i.profile_id,
                    "profile_name": i.profile_name,
                    "pid": i.pid,
                    "headless": i.headless,
                    "started_at": i.started_at,
                    "user_data_dir": i.user_data_dir,
                }
                for i in self.instances.values()
            ]
        return {"instances": items}
