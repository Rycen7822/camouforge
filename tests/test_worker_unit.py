#!/usr/bin/env python3
"""CamouForge worker 无浏览器单元测试。

覆盖 translate_profile 的 kwargs 展开规则与 _save_download 的文件名净化，
不需要真实浏览器/显示器。必须用项目 venv 的 Python 运行（依赖 camoufox/browserforge）：
    .venv/Scripts/python.exe tests/test_worker_unit.py
若 venv 装有 pytest，也可直接 `python -m pytest tests/test_worker_unit.py`。
"""

import importlib.util
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

WORKER = Path(__file__).resolve().parent.parent / "worker" / "camoforge_worker.py"

_spec = importlib.util.spec_from_file_location("camoforge_worker", WORKER)
w = importlib.util.module_from_spec(_spec)
assert _spec and _spec.loader
_spec.loader.exec_module(w)

import browser_patches as browser_patches


def make_profile(**overrides):
    launch = {"os": ["macos"], "headless": "headless"}
    launch.update(overrides.pop("launch", {}))
    prof = {
        "id": "unit-1",
        "name": "Unit",
        "notes": "",
        "created_at": 0,
        "updated_at": 0,
        "launch": launch,
        "config": {},
    }
    prof.update(overrides)
    return prof



def test_os_single_flattened_to_str():
    kw = w.translate_profile(make_profile(launch={"os": ["macos"]}), Path("/tmp"))
    assert kw["os"] == "macos"


def test_os_list_kept_as_list():
    kw = w.translate_profile(make_profile(launch={"os": ["macos", "windows"]}), Path("/tmp"))
    assert kw["os"] == ["macos", "windows"]


def test_os_invalid_rejected():
    try:
        w.translate_profile(make_profile(launch={"os": ["windows98"]}), Path("/tmp"))
    except ValueError:
        return
    raise AssertionError("invalid OS should raise ValueError")


def test_headless_modes():
    kw = w.translate_profile(make_profile(launch={"headless": "headless"}), Path("/tmp"))
    assert kw["headless"] is True
    kw = w.translate_profile(make_profile(launch={"headless": "virtual"}), Path("/tmp"))
    assert kw["headless"] == "virtual"
    kw = w.translate_profile(make_profile(launch={"headless": "headed"}), Path("/tmp"))
    assert "headless" not in kw


def test_window_default_headed_1280x720():
    kw = w.translate_profile(make_profile(launch={"headless": "headed"}), Path("/tmp"))
    assert kw.get("window") == (1280, 720)


def test_window_explicit_tuple():
    kw = w.translate_profile(make_profile(launch={"window": [1024, 768]}), Path("/tmp"))
    assert kw["window"] == (1024, 768)


def test_proxy_and_port():
    prof = make_profile(launch={"proxy": {"server": "http://127.0.0.1:8080", "port": 1080}})
    kw = w.translate_profile(prof, Path("/tmp"))
    assert kw["proxy"] == {"server": "http://127.0.0.1:8080"}
    assert kw["config"]["port"] == 1080


def test_geoip_modes():
    kw = w.translate_profile(make_profile(launch={"geoip": {"mode": "off"}}), Path("/tmp"))
    assert "geoip" not in kw
    kw = w.translate_profile(make_profile(launch={"geoip": {"mode": "auto"}}), Path("/tmp"))
    assert kw["geoip"] is True
    kw = w.translate_profile(make_profile(launch={"geoip": {"mode": "ip", "ip": "1.2.3.4"}}), Path("/tmp"))
    assert kw["geoip"] == "1.2.3.4"


def test_locale_single_flattened():
    kw = w.translate_profile(make_profile(launch={"locale": ["zh-CN"]}), Path("/tmp"))
    assert kw["locale"] == "zh-CN"
    kw = w.translate_profile(make_profile(launch={"locale": ["zh-CN", "en-US"]}), Path("/tmp"))
    assert kw["locale"] == ["zh-CN", "en-US"]


def test_exclude_addons_ubo_shortname():
    w.SDK.load()
    kw = w.translate_profile(make_profile(launch={"exclude_addons": ["ubo"]}), Path("/tmp"))
    assert kw["exclude_addons"] == [w.SDK.default_addons.DefaultAddons.UBO]


def test_persistent_context_default_user_data_dir():
    kw = w.translate_profile(make_profile(launch={"persistent_context": True}), Path("/tmp"))
    assert kw["persistent_context"] is True
    assert kw["user_data_dir"] == str(Path("/tmp") / "unit-1")


def test_firefox_user_prefs_and_extra_passthrough():
    prof = make_profile(launch={
        "firefox_user_prefs": {"browser.tabs.unloadOnLowMemory": True},
        "extra_launch_options": {"foo": 1},
    })
    kw = w.translate_profile(prof, Path("/tmp"))
    assert kw["firefox_user_prefs"]["browser.tabs.unloadOnLowMemory"] is True
    assert kw["foo"] == 1


def test_fingerprint_strips_prefixes_and_writes_dpr():
    w.SDK.load()
    fp = w.generate_fingerprint({"os": "macos"})["fingerprint"]
    prof = make_profile(
        launch={"fingerprint": fp},
        config={
            "navigator.platform": "MacIntel",
            "screen.width": 1920,
            "window.innerWidth": 1920,
            "headers.User-Agent": "Mozilla/5.0",
            "locale:language": "zh",
            "timezone": "Asia/Tokyo",  # 不在剥离前缀清单，应保留
        },
    )
    kw = w.translate_profile(prof, Path("/tmp"))
    cfg = kw["config"]
    stripped = ("navigator.", "screen.", "headers.User-Agent", "locale:")
    assert not any(k.startswith(stripped) for k in cfg), cfg
    # 用户填的 window.innerWidth 必须被剥离；只允许刻意回写的 window.devicePixelRatio
    window_keys = [k for k in cfg if k.startswith("window.")]
    assert window_keys == ["window.devicePixelRatio"], window_keys
    assert cfg["timezone"] == "Asia/Tokyo"
    dpr = (fp.get("screen") or {}).get("devicePixelRatio")
    if dpr:
        assert cfg["window.devicePixelRatio"] == dpr


def test_macos_font_lists_keep_apple_color_emoji():
    explicit = w.translate_profile(
        make_profile(launch={"os": ["macos"], "fonts": ["Helvetica Neue"]}),
        Path("/tmp"),
    )
    assert explicit["fonts"] == ["Helvetica Neue", "Apple Color Emoji"]

    fp = w.generate_fingerprint({"os": "macos"})["fingerprint"]
    fp["fonts"] = ["Helvetica Neue"]
    rebuilt = w.translate_profile(
        make_profile(launch={"os": ["macos"], "fingerprint": fp}),
        Path("/tmp"),
    )
    assert "Apple Color Emoji" in rebuilt["fonts"]
    assert len(rebuilt["fonts"]) > 20


def test_windows_font_list_does_not_gain_apple_color_emoji():
    kw = w.translate_profile(
        make_profile(launch={"os": ["windows"], "fonts": ["Arial"]}),
        Path("/tmp"),
    )
    assert kw["fonts"] == ["Arial"]


def test_emoji_font_bounds_are_recovered_without_changing_valid_values():
    damaged = {
        "head": SimpleNamespace(xMin=0, yMin=0, xMax=0, yMax=0),
        "hhea": SimpleNamespace(
            ascent=1900,
            descent=-500,
            advanceWidthMax=2550,
            xMaxExtent=0,
        ),
    }
    browser_patches._restore_font_bounds(damaged)
    assert (damaged["head"].xMin, damaged["head"].yMin) == (0, -500)
    assert (damaged["head"].xMax, damaged["head"].yMax) == (2550, 1900)
    assert damaged["hhea"].xMaxExtent == 2550

    valid = {
        "head": SimpleNamespace(xMin=-10, yMin=-20, xMax=30, yMax=40),
        "hhea": SimpleNamespace(
            ascent=100,
            descent=-40,
            advanceWidthMax=120,
            xMaxExtent=90,
        ),
    }
    browser_patches._restore_font_bounds(valid)
    assert (valid["head"].xMin, valid["head"].yMin) == (-10, -20)
    assert (valid["head"].xMax, valid["head"].yMax) == (30, 40)
    assert valid["hhea"].xMaxExtent == 90


def test_svg_emoji_images_require_firefox_xlink():
    document = SimpleNamespace(
        data=(
            '<svg xmlns="http://www.w3.org/2000/svg"><g id="glyph1">'
            '<image href="data:image/png;base64,AAAA"/></g></svg>'
        )
    )
    font = {"SVG ": SimpleNamespace(docList=[document])}
    assert not browser_patches._svg_images_are_firefox_compatible(font)

    document.data = (
        '<svg version="1.1" xmlns="http://www.w3.org/2000/svg" '
        'xmlns:xlink="http://www.w3.org/1999/xlink"><g id="glyph1">'
        '<image xlink:href="data:image/png;base64,AAAA"/></g></svg>'
    )

    assert browser_patches._svg_images_are_firefox_compatible(font)


def test_emoji_repair_source_is_outside_bundled_fonts_directory():
    browser_dir = Path("C:/camoufox")
    source = (
        browser_dir
        / browser_patches._MACOS_EMOJI_REPAIR_DIR
        / browser_patches._MACOS_EMOJI_SOURCE
    )
    assert source.parent != browser_dir / "fonts"


class FakeDownload:
    def __init__(self, name):
        self.suggested_filename = name

    def save_as(self, path):
        self.saved = str(path)


def _save_and_return(name, tmp):
    d = Path(tmp) / "dl"
    d.mkdir(exist_ok=True)
    dl = FakeDownload(name)
    w._save_download(dl, d)
    return Path(dl.saved), d


def test_save_download_sanitizes_traversal():
    cases = (
        "..\\..\\evil.exe",
        "..\\..\\..\\Windows\\System32\\evil.dll",
        "/etc/passwd",
        "C:\\evil.exe",
    )
    for evil in cases:
        with tempfile.TemporaryDirectory() as tmp:
            saved, d = _save_and_return(evil, tmp)
            assert saved.parent == d, f"{evil!r} escaped: {saved}"
            assert saved.name == Path(evil.replace("\\", "/")).name


def test_save_download_normal_name_kept():
    with tempfile.TemporaryDirectory() as tmp:
        saved, d = _save_and_return("report.pdf", tmp)
        assert saved == d / "report.pdf"


def test_save_download_empty_name_falls_back():
    with tempfile.TemporaryDirectory() as tmp:
        saved, d = _save_and_return("   ", tmp)
        assert saved == d / "download"



def test_stop_all_aggregates_partial_failures():
    mgr = w.Manager.__new__(w.Manager)  # 跳过 __init__（不建 user_data 目录）
    mgr.instances = {"a": object(), "b": object(), "c": object()}
    mgr.big_lock = w.threading.Lock()

    def fake_stop(pid):
        if pid == "b":
            return {"profile_id": pid, "stopped": False, "reason": "stop timeout (30s)"}
        if pid == "c":
            raise RuntimeError("boom")
        return {"profile_id": pid, "stopped": True}

    mgr.stop = fake_stop
    r = mgr.stop_all()
    assert r["stopped"] == ["a"], r
    assert r["failed"]["b"].startswith("stop timeout"), r
    assert r["failed"]["c"].startswith("RuntimeError"), r


def test_stop_all_empty():
    mgr = w.Manager.__new__(w.Manager)
    mgr.instances = {}
    mgr.big_lock = w.threading.Lock()
    assert mgr.stop_all() == {"stopped": [], "failed": {}}


def _with_fake_tree(tmp, fn):
    """构造假的 worker 目录树后调用 fn(tmp)。"""
    import os
    proj = Path(tmp) / "proj"
    (proj / "worker").mkdir(parents=True)
    old_file = w.sdk_bridge.__file__
    old_env = os.environ.pop("CAMOUFOX_EXECUTABLE_PATH", None)
    w.sdk_bridge.__file__ = str(proj / "worker" / "sdk_bridge.py")
    try:
        fn(Path(tmp))
    finally:
        w.sdk_bridge.__file__ = old_file
        if old_env is not None:
            os.environ["CAMOUFOX_EXECUTABLE_PATH"] = old_env


def test_default_executable_path_prefers_exact_name():
    exe = "camoufox.exe" if w.os.name == "nt" else "camoufox"

    def go(tmp):
        for d in ("camoufox-150", "camoufox-152"):
            (tmp / d).mkdir()
            (tmp / d / exe).write_text("")
        # 无精确名时取名称最大（最新版本目录），而非字母序最小
        got = w._default_executable_path()
        assert Path(got).resolve() == (tmp / "camoufox-152" / exe).resolve(), got
        # 精确名 camoufox 优先于任何版本化目录
        (tmp / "camoufox").mkdir()
        (tmp / "camoufox" / exe).write_text("")
        got = w._default_executable_path()
        assert Path(got).resolve() == (tmp / "camoufox" / exe).resolve(), got

    with tempfile.TemporaryDirectory() as tmp:
        _with_fake_tree(tmp, go)


def test_chrome_css_patches_append_incrementally_and_are_idempotent():
    with tempfile.TemporaryDirectory() as tmp:
        exe = Path(tmp) / "camoufox.exe"
        exe.write_text("")
        css = Path(tmp) / "chrome.css"
        css.write_text(browser_patches._TITLEBAR_SVG_CSS, encoding="utf-8")
        browser_patches._ensure_chrome_css_patches(str(exe))
        t1 = css.read_text(encoding="utf-8")
        assert t1.count(browser_patches._TITLEBAR_SVG_MARKER) == 1
        assert browser_patches._TAB_CLICK_MARKER in t1
        assert browser_patches._TAB_LAYOUT_MARKER in t1
        assert browser_patches._CHROME_DARK_THEME_MARKER in t1
        assert "-moz-window-dragging: no-drag" in t1
        assert "pointer-events: auto" in t1
        assert ".tabbrowser-tab[fadein]:not([pinned])" in t1
        assert "max-width: 240px" in t1
        assert "#TabsToolbar" in t1
        assert "background-color: #1f2020" in t1
        assert "background-color: #3c3c3c" in t1
        browser_patches._ensure_chrome_css_patches(str(exe))
        assert css.read_text(encoding="utf-8") == t1


def test_shortcuts_page_uses_chrome_dark_background():
    html = browser_patches._build_shortcuts_html(
        [{"name": "Example", "url": "https://example.com"}]
    )
    assert "background: #3c3c3c" in html
    assert "color: #e8eaed" in html


def test_chrome_css_patches_missing_file_noop():
    with tempfile.TemporaryDirectory() as tmp:
        exe = Path(tmp) / "camoufox.exe"
        exe.write_text("")
        browser_patches._ensure_chrome_css_patches(str(exe))
        assert not (Path(tmp) / "chrome.css").exists()


def test_pid_alive():
    import os
    import subprocess
    assert w._pid_alive(os.getpid())
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    assert not w._pid_alive(p.pid)


def test_manager_process_helpers_are_bound():
    manager = sys.modules[w.Manager.__module__]
    assert callable(manager._pid_alive)
    if w.os.name == "nt":
        assert callable(manager._find_browser_pid)
        assert callable(manager._tree_has_visible_window)


def test_process_tree_helpers():
    if w.os.name != "nt":
        return
    import os
    import subprocess
    import time
    me = os.getpid()
    assert me in w._process_snapshot()
    p = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
    try:
        bp = None
        for _ in range(40):
            bp = w._find_browser_pid(me, Path(sys.executable).name)
            if bp:
                break
            time.sleep(0.1)
        assert bp == p.pid
        assert p.pid in w._tree_pids(me, w._process_snapshot())
    finally:
        p.kill()


def _run_all():
    tests = [
        (name, fn) for name, fn in sorted(globals().items())
        if name.startswith("test_") and callable(fn)
    ]
    failed = []
    for name, fn in tests:
        try:
            fn()
        except Exception as e:
            failed.append((name, e))
            print(f"  FAIL {name}: {type(e).__name__}: {e}")
    print(f"\npassed: {len(tests) - len(failed)} failed: {len(failed)}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(_run_all())
