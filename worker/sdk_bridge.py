from __future__ import annotations

import dataclasses
import os
import re
import sys
import threading
from dataclasses import asdict
from pathlib import Path
from typing import Any, Dict, Optional

class _Sdk:
    """延迟导入 SDK（ping 在浏览器未安装时也应可用）。"""

    def __init__(self) -> None:
        self._loaded = False
        self.camoufox: Any = None
        self.camoufox_fp: Any = None
        self.warnings: Any = None
        self.default_addons: Any = None

    def load(self) -> None:
        if self._loaded:
            return
        _relocate_sdk_cache()
        import camoufox
        import camoufox.async_api
        import camoufox.fingerprints
        import camoufox._warnings
        import camoufox.addons
        self.camoufox = camoufox
        self.camoufox_fp = camoufox.fingerprints
        self.warnings = camoufox._warnings
        self.default_addons = camoufox.addons
        self._loaded = True


SDK = _Sdk()


VALID_OS = ("windows", "macos", "linux")


def _rebuild_fingerprint(d: Dict[str, Any]) -> Any:
    from browserforge.fingerprints import Fingerprint, ScreenFingerprint, NavigatorFingerprint, VideoCard

    def filtered(cls, src):
        if src is None:
            return None
        names = {f.name for f in dataclasses.fields(cls)}
        return cls(**{k: v for k, v in src.items() if k in names})

    return Fingerprint(
        screen=filtered(ScreenFingerprint, d.get("screen")),
        navigator=filtered(NavigatorFingerprint, d.get("navigator")),
        headers=d.get("headers") or {},
        videoCodecs=d.get("videoCodecs") or {},
        audioCodecs=d.get("audioCodecs") or {},
        pluginsData=d.get("pluginsData") or {},
        battery=d.get("battery"),
        videoCard=filtered(VideoCard, d.get("videoCard")),
        multimediaDevices=d.get("multimediaDevices") or [],
        fonts=d.get("fonts") or [],
        mockWebRTC=d.get("mockWebRTC"),
        slim=d.get("slim"),
    )


def _apply_window_to_fingerprint(fp: Any, size: tuple) -> None:
    """把目标外窗尺寸写进指纹 screen（居中），让真实窗口与上报值一致。

    SDK 在传入 fingerprint 时忽略 window 参数，窗口尺寸由指纹 screen 决定；
    用 camoufox 自带的 handle_window_size 保证 inner/outer/screen 几何自洽。
    """
    try:
        from camoufox.fingerprints import handle_window_size
        handle_window_size(fp, int(size[0]), int(size[1]))
    except Exception:
        pass


def _infer_ff_version(executable_path: str) -> Optional[int]:
    """从浏览器目录的 application.ini 读取 Firefox 主版本号。

    camoufox 在 ff_version 为空时会去查默认安装目录（official/stable），
    自定义 executable_path 时若未指定 ff_version 会抛 CamoufoxNotInstalled。
    """
    import configparser

    p = Path(executable_path)
    ini_path = p / "application.ini" if p.is_dir() else p.parent / "application.ini"
    if not ini_path.exists():
        return None
    try:
        ini = configparser.ConfigParser()
        ini.read(ini_path, encoding="utf-8")
        m = re.match(r"(\d+)", ini.get("App", "Version", fallback=""))
        return int(m.group(1)) if m else None
    except Exception:
        return None


def _default_executable_path() -> Optional[str]:
    """探测手动下载的 camoufox 浏览器（默认 SDK 安装目录之外）。

    优先级：CAMOUFOX_EXECUTABLE_PATH 环境变量（目录或 exe 路径）> camouforge 兄弟目录。
    兄弟目录中精确名 "camoufox" 优先；版本化目录（camoufox-150/-152…）按名称倒序
    取最新，避免字母序把旧版本排前面。
    """
    exe_name = "camoufox.exe" if os.name == "nt" else "camoufox"
    env = os.environ.get("CAMOUFOX_EXECUTABLE_PATH")
    if env:
        p = Path(env)
        if p.is_dir():
            cand = p / exe_name
            if cand.exists():
                return str(cand)
        elif p.exists():
            return env
    worker_dir = Path(__file__).resolve().parent  # <root>/worker
    root_dir = worker_dir.parent  # <root>（camouforge）
    sibling = root_dir.parent
    for scan_dir in (root_dir, sibling):
        if not scan_dir.exists():
            continue
        cands = [
            d for d in scan_dir.iterdir()
            if d.is_dir() and d.name.startswith("camoufox") and (d / exe_name).exists()
        ]
        chosen = next((d for d in cands if d.name == "camoufox"), None)
        if chosen is None and cands:
            chosen = max(cands, key=lambda d: d.name)
        if chosen is not None:
            exe = str(chosen / exe_name)
            print(f"[camoforge] 自动探测浏览器: {exe}", file=sys.stderr)
            return exe
    for candidate in (
        root_dir / "camoufox" / exe_name,
        worker_dir / "camoufox" / exe_name,
    ):
        if candidate.exists():
            return str(candidate)
    return None



def _norm_os(value):
    if not value:
        return None
    if isinstance(value, str):
        return value
    if len(value) == 1:
        return value[0]
    return value


def _rewrite_firefox_version(ua: str, ff_version: int) -> str:
    ua = re.sub(r'rv:\d+\.0', f'rv:{ff_version}.0', ua)
    ua = re.sub(r'Firefox/\d+\.0', f'Firefox/{ff_version}.0', ua)
    return ua


def generate_fingerprint(params: Dict[str, Any]) -> Dict[str, Any]:
    SDK.load()
    gen_kw: Dict[str, Any] = {}
    os_arg = _norm_os(params.get("os"))
    if os_arg:
        gen_kw["os"] = os_arg
    if params.get("locale"):
        gen_kw["locale"] = params["locale"]
    window = params.get("window")
    fp = SDK.camoufox_fp.generate_fingerprint(window=tuple(window) if window else None, **gen_kw)

    # camoufox 0.5.5 的 generate_fingerprint 不支持 ff_version（browserforge 默认生成
    # Firefox 150 的 UA）。这里从实际浏览器推断主版本，并把 UA 版本号对齐过去，
    # 避免与本地 152 浏览器不一致触发「Spoofing the Firefox version」告警。
    ff_version = params.get("ff_version")
    if not ff_version:
        exe = params.get("executable_path") or _default_executable_path()
        if exe:
            ff_version = _infer_ff_version(exe)

    d = asdict(fp)
    nav = d.get("navigator") or {}
    if ff_version:
        if nav.get("userAgent"):
            nav["userAgent"] = _rewrite_firefox_version(nav["userAgent"], ff_version)
        hdrs = d.get("headers") or {}
        if hdrs.get("User-Agent"):
            hdrs["User-Agent"] = _rewrite_firefox_version(hdrs["User-Agent"], ff_version)
    screen = d.get("screen") or {}
    vc = d.get("videoCard") or {}
    return {
        "fingerprint": d,
        "summary": {
            "user_agent": nav.get("userAgent", ""),
            "os": (params.get("os") or ["auto"])[0] if isinstance(params.get("os"), list) else (params.get("os") or "auto"),
            "platform": nav.get("platform", ""),
            "screen": f"{screen.get('width')}x{screen.get('height')}",
            "hardware_concurrency": nav.get("hardwareConcurrency", 0),
            "device_memory": nav.get("deviceMemory"),
            "webgl_vendor": vc.get("vendor", ""),
            "webgl_renderer": vc.get("renderer", ""),
            "languages": [nav.get("language")] if nav.get("language") else [],
        },
    }

def list_webgl(params: Dict[str, Any]) -> Dict[str, Any]:
    """按 OS 列出 webgl_config 可用的 vendor/renderer 组合（含市场权重）。

    webgl_config 参数必须是数据库中的精确字符串，否则 SDK 抛
    "No WebGL data found"——UI 用此方法提供选择器。
    """
    import sqlite3
    import camoufox as _camoufox
    os_name = params.get("os", "macos")
    col = {"windows": "win", "macos": "mac", "linux": "lin"}.get(os_name, "mac")
    db_path = Path(_camoufox.__file__).parent / "webgl" / "webgl_data.db"
    if not db_path.exists():
        return {"cards": []}
    db = sqlite3.connect(str(db_path))
    try:
        rows = db.execute(
            f"SELECT vendor, renderer, {col} FROM webgl_fingerprints WHERE {col} > 0 ORDER BY {col} DESC"
        ).fetchall()
    finally:
        db.close()
    cards = [
        {"vendor": v, "renderer": r, "weight": round(w, 4)}
        for (v, r, w) in rows
        if w and w > 0
    ]
    return {"cards": cards}


def list_versions(params: Dict[str, Any]) -> Dict[str, Any]:
    SDK.load()
    try:
        from camoufox.multiversion import list_installed
        installed = list_installed()
    except Exception:
        installed = []
    versions = [f"{v.repo_name}/{v.version.full_string}" for v in installed]
    return {"versions": versions}


def _list_os_catalog(params: Dict[str, Any], filename: str) -> Dict[str, Any]:
    import json as _json
    import camoufox as _camoufox
    os_name = params.get("os", "macos")
    key = {"windows": "win", "macos": "mac", "linux": "lin"}.get(os_name, "mac")
    p = Path(_camoufox.__file__).parent / filename
    if not p.exists():
        return {"items": []}
    data = _json.loads(p.read_text(encoding="utf-8"))
    return {"items": data.get(key, [])}


def list_fonts(params: Dict[str, Any]) -> Dict[str, Any]:
    items = _list_os_catalog(params, "fonts.json")["items"]
    return {"fonts": items}


def list_voices(params: Dict[str, Any]) -> Dict[str, Any]:
    items = _list_os_catalog(params, "voices.json")["items"]
    return {"voices": items}

def _preload_sdk() -> None:
    """后台预热异步 SDK；失败留给实际调用处理。"""
    try:
        SDK.load()
        import browserforge.fingerprints  # noqa: F401
    except Exception:
        pass


_SDK_CACHE_RELOCATE_LOCK = threading.Lock()
_SDK_CACHE_RELOCATED = False


def _relocate_sdk_cache() -> None:
    """把 camoufox SDK 的缓存目录（geoip 数据库、默认插件 UBO）重定向到便携数据目录。

    SDK 在 import 时把 addons.ADDONS_DIR、geolocation.GEOIP_DIR/MMDB_DIR/GEOIP_CONFIG
    钉死在 user_cache_dir("camoufox")（Windows 固定 %LOCALAPPDATA%，无环境变量开关）。
    触发点：_Sdk.load() 与 Manager.launch（launch 不经过 SDK.load()）；锁 + 幂等标记
    防预热线程与 RPC 线程并发双跑。SDK 升级后属性名变化则落回默认位置，只影响缓存位置。
    """
    global _SDK_CACHE_RELOCATED
    with _SDK_CACHE_RELOCATE_LOCK:
        if _SDK_CACHE_RELOCATED:
            return
        root = Path(os.environ.get("CAMOFORGE_DATA_DIR", Path.home() / ".camoforge")) / "cache"
        try:
            root.mkdir(parents=True, exist_ok=True)
            import camoufox.addons
            import camoufox.geolocation
            import camoufox.pkgman

            camoufox.pkgman.INSTALL_DIR = root
            camoufox.addons.ADDONS_DIR = root / "addons"
            geo = root / "geoip"
            camoufox.geolocation.GEOIP_DIR = geo
            camoufox.geolocation.MMDB_DIR = geo / "mmdb"
            camoufox.geolocation.GEOIP_CONFIG = geo / "config.yml"
            print(f"[camoforge] SDK 缓存目录: {root}", file=sys.stderr)
        except Exception as e:
            print(f"[camoforge] SDK 缓存重定向失败（沿用默认位置）: {e}", file=sys.stderr)
        finally:
            _SDK_CACHE_RELOCATED = True
