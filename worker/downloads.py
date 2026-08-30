from __future__ import annotations

import asyncio
import sys
from pathlib import Path
from typing import Any, Dict, Optional, Set


def _resolve_downloads_dir(profile: Dict[str, Any]) -> Path:
    lo = profile.get("launch") or {}
    d = lo.get("downloads_dir")
    if d:
        return Path(d).expanduser()
    return Path.home() / "Downloads"


def _unique_download_target(
    downloads_dir: Path, name: str, reserved: Optional[Set[Path]] = None
) -> Path:
    if reserved is None:
        reserved = set()
    target = downloads_dir / name
    stem, suffix = target.stem, target.suffix
    i = 1
    while target.exists() or target in reserved:
        target = downloads_dir / f"{stem} ({i}){suffix}"
        i += 1
    return target


async def _save_download(
    dl: Any,
    downloads_dir: Path,
    name_lock: Optional[asyncio.Lock] = None,
    reserved: Optional[Set[Path]] = None,
) -> None:
    """异步保存下载，并阻止站点提供的文件名越出下载目录。"""
    target: Optional[Path] = None
    try:
        downloads_dir.mkdir(parents=True, exist_ok=True)
        raw = (getattr(dl, "suggested_filename", None) or "download").strip() or "download"
        name = Path(raw.replace("\\", "/")).name
        if name_lock is None:
            target = _unique_download_target(downloads_dir, name, reserved)
        else:
            async with name_lock:
                target = _unique_download_target(downloads_dir, name, reserved)
                if reserved is not None:
                    reserved.add(target)
        if not target.resolve().is_relative_to(downloads_dir.resolve()):
            raise ValueError(f"download target escapes downloads dir: {target}")
        await dl.save_as(str(target))
        print(f"[camoforge] 已下载: {target}", file=sys.stderr)
    except Exception as e:  # 下载失败不应拖垮浏览器实例
        print(f"[camoforge] 下载保存失败: {e}", file=sys.stderr)
    finally:
        if reserved is not None and target is not None:
            reserved.discard(target)


def _attach_download_handler(context: Any, inst: "Instance", downloads_dir: Path) -> None:
    def on_download(dl: Any) -> None:
        try:
            if not inst.accept_downloads:
                return
            print("[camoforge] 收到下载事件", file=sys.stderr)
            slots = inst.download_slots
            if slots is None:
                raise RuntimeError("download coordinator is not ready")

            async def save() -> None:
                async with slots:
                    await _save_download(
                        dl,
                        downloads_dir,
                        inst.download_name_lock,
                        inst.download_targets,
                    )

            task = asyncio.create_task(save())
            inst.download_tasks.add(task)
            task.add_done_callback(inst.download_tasks.discard)
        except Exception as e:
            print(f"[camoforge] 创建下载任务失败: {e}", file=sys.stderr)

    try:
        context.on("download", on_download)
        print("[camoforge] 下载处理器已挂接", file=sys.stderr)
    except Exception as e:
        print(f"[camoforge] 挂下载处理器失败: {e}", file=sys.stderr)


def _attach_close_watcher(browser: Any, inst: "Instance") -> None:
    """浏览器或上下文关闭时唤醒实例循环。"""

    def on_gone(*_a: Any) -> None:
        inst.notify_exit("浏览器已退出（窗口被关闭或进程结束）")

    for ev in ("disconnected", "close"):
        try:
            browser.on(ev, on_gone)
        except Exception:
            pass
