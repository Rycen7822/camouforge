from __future__ import annotations

import os
import queue
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
    _pump_playwright,
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
        self.q: "queue.Queue[Optional[Any]]" = queue.Queue()
        self.ready = threading.Event()  # 启动完成/失败信号（独立于命令队列）
        self.response_sent = threading.Event()  # launch 响应已 flush（Manager.launch_done 置位）
        self.browser_gone = threading.Event()  # 浏览器进程退出（用户直接关窗等）
        self.stop_requested = False  # 经由 stop 命令的正常停止
        self.lock = threading.Lock()
        self.alive = True
        self.thread: Optional[threading.Thread] = None
        self.exit_reason: Optional[str] = None

    def submit(self, fn):
        self.q.put(fn)


def _now() -> int:
    return int(time.time())


class Manager:
    def __init__(self, emit):
        self.emit = emit  # 事件回调: emit(event, data)
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

        def run(inst: Instance):
            browser = None
            launched = False
            try:
                from camoufox.sync_api import Camoufox
                cm = Camoufox(**kw)
                browser = cm.__enter__()
                launched = True
                # 下载落盘 + 初始页：持久化上下文时 browser 即 BrowserContext
                # （accept_downloads 已随 launch_persistent_context 传入，enter 后
                # browser.pages 已有 1 页）；非持久化时 browser 是 Browser，须显式
                # new_context(accept_downloads=True) 并把 handler 挂到 context——否则
                # 下载被 Juggler 静默丢弃，且 Browser 本身没有 download 事件。
                persistent = bool(kw.get("persistent_context"))
                page = None
                try:
                    if persistent:
                        pages = getattr(browser, "pages", None)
                        page = pages[0] if pages else browser.new_page()
                        _attach_download_handler(browser, inst, downloads_dir)
                        _attach_close_watcher(browser, inst)
                    else:
                        context = browser.new_context(accept_downloads=True)
                        _attach_download_handler(context, inst, downloads_dir)
                        _attach_close_watcher(context, inst)
                        _attach_close_watcher(browser, inst)
                        page = context.new_page()
                except Exception as e:
                    print(f"[camoforge] 初始页/下载处理器失败: {e}", file=sys.stderr)
                try:
                    inst.pid = browser._impl_obj._connection._transport._proc.pid  # type: ignore[attr-defined]
                except Exception:
                    inst.pid = None
                # inst.pid 是 node 驱动；真实浏览器主进程是其直接子进程。
                # 关窗/崩溃检测以主进程为准（驱动在浏览器死后仍可能存活）。
                if os.name == "nt" and inst.pid:
                    exe_name = Path(kw["executable_path"]).name if kw.get("executable_path") else "camoufox.exe"
                    for _ in range(20):
                        inst.browser_pid = _find_browser_pid(inst.pid, exe_name)
                        if inst.browser_pid:
                            break
                        time.sleep(0.25)
                if page is not None and shortcut_page:
                    try:
                        page.goto(shortcut_page, wait_until="domcontentloaded", timeout=15_000)
                    except Exception:
                        pass
                inst.ready.set()  # 启动完成，解除主线程等待
                # 关窗检测：headed 模式下见过窗口后又失去全部可见窗口（且进程仍在）
                # = 用户关窗——camoufox 进程树关窗后会无窗口常驻，disconnected 不会发。
                window_seen = False
                windowless_since: Optional[float] = None
                last_watch = 0.0
                # 命令循环（同线程内操作 sync playwright 对象）。下载落盘任务也走这里，
                # 保证 save_as 不在事件回调里调用。空闲时泵调度器，否则用户点击下载
                # 时事件永远交不到 Python；泵的同时也让 disconnected/close 事件送达。
                while True:
                    if inst.browser_gone.is_set():
                        break
                    now = time.monotonic()
                    if now - last_watch >= 0.5:
                        last_watch = now
                        watch_pid = inst.browser_pid or inst.pid
                        if watch_pid is not None and not _pid_alive(watch_pid):
                            with inst.lock:
                                if not inst.stop_requested and inst.exit_reason is None:
                                    inst.exit_reason = "浏览器已退出（窗口被关闭或进程结束）"
                            break
                        if (
                            os.name == "nt"
                            and not inst.headless
                            and inst.browser_pid is not None
                        ):
                            if _tree_has_visible_window(inst.browser_pid):
                                window_seen = True
                                windowless_since = None
                            elif window_seen:
                                windowless_since = windowless_since or now
                                if now - windowless_since >= 1.0:
                                    with inst.lock:
                                        if not inst.stop_requested and inst.exit_reason is None:
                                            inst.exit_reason = "浏览器窗口已关闭"
                                    break
                    try:
                        task = inst.q.get(timeout=0.05)
                    except queue.Empty:
                        _pump_playwright(browser)
                        continue
                    if task is None:
                        with inst.lock:
                            inst.stop_requested = True
                        break
                    try:
                        task(browser)
                    except Exception as e:
                        print(f"[camoforge] 实例任务失败: {e}", file=sys.stderr)
            except Exception as e:  # 启动失败或运行期异常
                reason = f"{type(e).__name__}: {e}"
                with inst.lock:
                    inst.exit_reason = reason
                    inst.alive = False
                traceback.print_exc(file=sys.stderr)
                inst.ready.set()  # 若主线程还在等启动，解除阻塞
            finally:
                if browser is not None:
                    try:
                        browser.close()
                    except Exception:
                        pass
                with inst.lock:
                    inst.alive = False
                with self.big_lock:
                    self.instances.pop(inst.profile_id, None)
                if launched:
                    # 先等 launch 响应发出再发 instance_exited：浏览器「刚启动就退出」时
                    # 事件若先于响应到达 Rust 端，UI 先 remove 后 insert，留下永远清不掉
                    # 的幽灵运行条目。正常情况响应早已发出，wait 立即返回。
                    inst.response_sent.wait(timeout=2.0)
                    self.emit("instance_exited", {"profile_id": inst.profile_id,
                                                  "reason": inst.exit_reason or "stopped",
                                                  "started": True})

        inst.thread = threading.Thread(target=run, args=(inst,), daemon=True, name=f"camoforge-{pid[:8]}")
        inst.thread.start()

        # 等待启动完成或失败（最长 180s：首次可能要解压浏览器）
        if not inst.ready.wait(timeout=180):
            with inst.lock:
                inst.exit_reason = "launch timeout (180s)"
                inst.alive = False
            inst.q.put(None)  # 让 run 线程退出命令循环
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
        inst.submit(None)  # 触发 run 循环退出 → with Camoufox 退出 → 浏览器关闭
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
