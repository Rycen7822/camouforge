from __future__ import annotations

import sys
import time
from pathlib import Path
from typing import Any, Dict

def _resolve_downloads_dir(profile: Dict[str, Any]) -> Path:
    lo = profile.get("launch") or {}
    d = lo.get("downloads_dir")
    if d:
        return Path(d).expanduser()
    return Path.home() / "Downloads"


def _unique_download_target(downloads_dir: Path, name: str) -> Path:
    target = downloads_dir / name
    stem, suffix = target.stem, target.suffix
    i = 1
    while target.exists():
        target = downloads_dir / f"{stem} ({i}){suffix}"
        i += 1
    return target


def _save_download(dl: Any, downloads_dir: Path) -> None:
    """在 Playwright 同步线程里把一次下载落盘。禁止从 download 事件回调里调用。

    suggested_filename 来自站点 Content-Disposition，恶意站点可塞 ``..\\..\\evil.exe``
    之类的路径段；这里只取 basename（先归一化反斜杠，跨平台一致），并在落盘前
    断言解析后的目标仍在下载目录内，防止把文件写到下载目录之外。
    """
    try:
        downloads_dir.mkdir(parents=True, exist_ok=True)
        raw = (getattr(dl, "suggested_filename", None) or "download").strip() or "download"
        name = Path(raw.replace("\\", "/")).name
        target = _unique_download_target(downloads_dir, name)
        if not target.resolve().is_relative_to(downloads_dir.resolve()):
            raise ValueError(f"download target escapes downloads dir: {target}")
        dl.save_as(str(target))
        print(f"[camoforge] 已下载: {target}", file=sys.stderr)
    except Exception as e:  # 下载失败不应拖垮浏览器实例
        print(f"[camoforge] 下载保存失败: {e}", file=sys.stderr)

def _first_page(browser: Any) -> Any:
    pages = getattr(browser, "pages", None)
    if pages:
        return pages[0]
    for ctx in getattr(browser, "contexts", None) or []:
        pages = getattr(ctx, "pages", None)
        if pages:
            return pages[0]
    return None

def _pump_playwright(browser: Any, timeout_ms: float = 200) -> None:
    """让 sync Playwright 调度器跑一小段，把积压的 download/page 事件交出来。

    实例线程空闲时若阻塞在 ``queue.get()`` 上，调度器停转：Juggler 已经拦截了
    下载，Python 却收不到 ``download`` 事件，文件被丢掉，Firefox 下载库也是空的。
    """
    page = _first_page(browser)
    if page is None:
        time.sleep(timeout_ms / 1000.0)
        return
    try:
        page.wait_for_timeout(timeout_ms)
    except Exception:
        time.sleep(timeout_ms / 1000.0)


def _attach_download_handler(context: Any, inst: "Instance", downloads_dir: Path) -> None:
    """只把 download 事件投递到实例命令队列。

    sync Playwright 的事件回调跑在调度器 greenlet 上：若在回调里调用
    ``dl.save_as()``，小文件可能碰巧已写完所以能成功，但真实下载还在传输时
    ``save_as`` 要等调度器继续收包——调度器却卡在回调里，形成死锁。
    """
    def on_download(dl: Any) -> None:
        try:
            print("[camoforge] 收到下载事件", file=sys.stderr)
            inst.submit(lambda _browser, d=dl: _save_download(d, downloads_dir))
        except Exception as e:
            print(f"[camoforge] 入队下载失败: {e}", file=sys.stderr)

    try:
        context.on("download", on_download)
        print("[camoforge] 下载处理器已挂接", file=sys.stderr)
    except Exception as e:
        print(f"[camoforge] 挂下载处理器失败: {e}", file=sys.stderr)


def _attach_close_watcher(browser: Any, inst: "Instance") -> None:
    """用户直接关浏览器窗口时，让命令循环退出并发 instance_exited。

    浏览器进程退出时 Playwright 对 Browser 发 ``disconnected``、对
    BrowserContext 发 ``close``（两个名字都挂，不认识的不会触发）。回调只置
    标志；命令循环在空闲泵调度器时看到标志即退出，由 finally 统一发事件，
    主窗口的「停止」按钮/状态栏才能回到未启动态——否则用户关窗后 UI 永远
    显示运行中。
    """

    def on_gone(*_a: Any) -> None:
        inst.browser_gone.set()
        with inst.lock:
            if not inst.stop_requested and inst.exit_reason is None:
                inst.exit_reason = "浏览器已退出（窗口被关闭或进程结束）"

    for ev in ("disconnected", "close"):
        try:
            browser.on(ev, on_gone)
        except Exception:
            pass
