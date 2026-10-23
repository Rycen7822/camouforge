#!/usr/bin/env python3
"""CamouForge worker 无浏览器单元测试。

覆盖 translate_profile 的 kwargs 展开规则与 _save_download 的文件名净化，
不需要真实浏览器/显示器。必须用项目 venv 的 Python 运行（依赖 camoufox/browserforge）：
    .venv/Scripts/python.exe tests/test_worker_unit.py
若 venv 装有 pytest，也可直接 `python -m pytest tests/test_worker_unit.py`。
"""

import asyncio
import importlib.util
import os
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parent.parent
TEST_TEMP = ROOT / "temp" / "worker-unit"
TEST_TEMP.mkdir(parents=True, exist_ok=True)
os.environ["TMP"] = os.environ["TEMP"] = str(TEST_TEMP)
tempfile.tempdir = str(TEST_TEMP)
_TEST_DATA = tempfile.TemporaryDirectory(prefix="data-", dir=TEST_TEMP)
os.environ["CAMOFORGE_DATA_DIR"] = _TEST_DATA.name
WORKER = ROOT / "worker" / "camoforge_worker.py"

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
    if os.name == "nt":
        assert kw["firefox_user_prefs"]["browser.nova.enabled"] is False


def test_window_default_headed_1280x720():
    kw = w.translate_profile(make_profile(launch={"headless": "headed"}), Path("/tmp"))
    assert kw.get("window") == (1280, 720)


def test_window_explicit_tuple():
    kw = w.translate_profile(make_profile(launch={"window": [1024, 768]}), Path("/tmp"))
    assert kw["window"] == (1024, 768)


def test_window_restores_native_size_and_keeps_explicit_override():
    import json

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        directory = root / "unit-1"
        directory.mkdir()
        store = directory / "xulstore.json"
        store.write_text(json.dumps({"chrome://browser/content/browser.xhtml": {
            "main-window": {"width": "1200", "height": "900", "sizemode": "normal"}
        }}), encoding="utf-8")
        profile = make_profile(launch={"headless": "headed", "persistent_context": True})
        assert w.translate_profile(profile, root)["window"] == (1200, 900)
        profile["launch"]["window"] = [1024, 768]
        assert w.translate_profile(profile, root)["window"] == (1024, 768)
        profile["launch"].pop("window")
        store.write_text("{", encoding="utf-8")
        assert w.translate_profile(profile, root)["window"] == (1280, 720)


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


def test_geoip_cache_hit_and_refresh():
    from unittest.mock import patch

    import camoufox.geolocation as geolocation
    import camoufox.utils as utils
    import geoip_cache
    from camoufox.locales import Geolocation, Locale

    w.SDK.load()
    assert utils.public_ip is geoip_cache.get_public_ip
    assert utils.get_geolocation is geoip_cache.get_cached_geolocation
    result = Geolocation(Locale("ja", "JP"), 139.7, 35.6, "Asia/Tokyo")
    with (
        tempfile.TemporaryDirectory() as tmp,
        patch.object(geolocation, "GEOIP_DIR", Path(tmp)),
        patch.object(geolocation, "get_geolocation", return_value=result) as lookup,
        patch.object(geoip_cache.time, "time", return_value=1000) as clock,
    ):
        def cached(ip):
            return geoip_cache.get_cached_geolocation(ip, "MaxMind GeoLite2")

        assert cached("192.0.2.1") == result
        clock.return_value = 1000 + geoip_cache._MAX_AGE - 1
        assert cached("192.0.2.1") == result
        assert lookup.call_count == 1
        clock.return_value += 1
        assert cached("192.0.2.1") == result
        assert lookup.call_count == 2
        assert cached("192.0.2.2") == result
        assert lookup.call_count == 3
        (Path(tmp) / "lookup.json").write_text("invalid json", encoding="utf-8")
        assert cached("192.0.2.2") == result
        assert lookup.call_count == 4


def test_exit_ip_uses_current_proxy_without_waiting_for_slow_service():
    import threading
    from unittest.mock import patch

    import geoip_cache

    release = threading.Event()

    def fetch(url, **kwargs):
        proxy = kwargs["proxies"]["https"]
        assert kwargs["proxies"]["http"] == proxy
        if url == "https://api.ipify.org":
            release.wait(2)
            ip = "192.0.2.99"
        else:
            ip = "192.0.2.1" if proxy.endswith(":8001") else "192.0.2.2"
        return SimpleNamespace(text=ip, raise_for_status=lambda: None)

    try:
        with patch("requests.get", side_effect=fetch):
            assert geoip_cache.get_public_ip("http://127.0.0.1:8001") == "192.0.2.1"
            assert geoip_cache.get_public_ip("http://127.0.0.1:8002") == "192.0.2.2"
    finally:
        release.set()


def test_exit_ip_rejects_invalid_results_instead_of_using_stale_ip():
    from unittest.mock import patch

    from camoufox.exceptions import InvalidIP
    import geoip_cache

    response = SimpleNamespace(text="999.0.0.1", raise_for_status=lambda: None)
    with patch("requests.get", return_value=response):
        try:
            geoip_cache.get_public_ip("http://127.0.0.1:8001")
        except InvalidIP:
            pass
        else:
            raise AssertionError("invalid exit IP was accepted")


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
    import json

    kw = w.translate_profile(make_profile(launch={"persistent_context": True}), Path("/tmp"))
    assert kw["persistent_context"] is True
    assert kw["user_data_dir"] == str(Path("/tmp") / "unit-1")
    assert kw["firefox_user_prefs"]["toolkit.legacyUserProfileCustomizations.stylesheets"] is True
    assert json.loads(kw["env"]["CAMOU_PREFS_1"]) == kw["firefox_user_prefs"]


def test_firefox_user_prefs_and_extra_passthrough():
    prof = make_profile(launch={
        "firefox_user_prefs": {"browser.tabs.unloadOnLowMemory": True},
        "extra_launch_options": {"foo": 1},
    })
    kw = w.translate_profile(prof, Path("/tmp"))
    assert kw["firefox_user_prefs"]["browser.tabs.unloadOnLowMemory"] is True
    assert kw["foo"] == 1
    assert prof["launch"]["firefox_user_prefs"] == {"browser.tabs.unloadOnLowMemory": True}


def test_macos_defaults_and_startup_locale_match_identity():
    import json

    prof = make_profile(launch={"locale": ["ja-JP", "en-US"]})
    kw = w.translate_profile(prof, Path("/tmp"))
    prefs = kw["firefox_user_prefs"]
    assert prefs["font.name.sans-serif.zh-CN"] == "PingFang SC"
    assert prefs["font.name.serif.zh-CN"] == "Songti SC"
    assert prefs["intl.locale.requested"] == "ja-JP,en-US"
    assert json.loads(kw["env"]["CAMOU_PREFS_1"]) == prefs
    assert kw["env"]["TMP"] == os.environ["TMP"]
    assert "Songti SC" in kw["fonts"]


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


def test_svg_emoji_validation_is_cached_by_file_metadata():
    with tempfile.TemporaryDirectory() as tmp:
        font = Path(tmp) / "fonts" / "AppleColorEmoji.ttf"
        font.parent.mkdir()
        font.write_bytes(b"first")
        calls = []
        original = browser_patches._font_has_svg_emoji
        browser_patches._cached_font_has_svg_emoji.cache_clear()
        browser_patches._font_has_svg_emoji = lambda path: calls.append(path) or True
        try:
            assert browser_patches._validated_svg_emoji(font)
            assert browser_patches._validated_svg_emoji(font)
            assert len(calls) == 1

            browser_patches._cached_font_has_svg_emoji.cache_clear()
            assert browser_patches._validated_svg_emoji(font)
            assert len(calls) == 1

            font.write_bytes(b"second-version")
            assert browser_patches._validated_svg_emoji(font)
            assert len(calls) == 2
        finally:
            browser_patches._font_has_svg_emoji = original
            browser_patches._cached_font_has_svg_emoji.cache_clear()


class FakeDownload:
    def __init__(self, name):
        self.suggested_filename = name

    async def save_as(self, path):
        self.saved = str(path)


def _save_and_return(name, tmp):
    d = Path(tmp) / "dl"
    d.mkdir(exist_ok=True)
    dl = FakeDownload(name)
    asyncio.run(w._save_download(dl, d))
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


def test_concurrent_downloads_reserve_unique_names():
    async def run(tmp):
        directory = Path(tmp)
        lock = asyncio.Lock()
        reserved = set()
        started = asyncio.Event()
        release = asyncio.Event()

        class BlockingDownload(FakeDownload):
            async def save_as(self, path):
                self.saved = str(path)
                started.set()
                await release.wait()

        first = BlockingDownload("report.pdf")
        second = FakeDownload("report.pdf")
        first_task = asyncio.create_task(
            w._save_download(first, directory, lock, reserved)
        )
        await started.wait()
        await w._save_download(second, directory, lock, reserved)
        release.set()
        await first_task
        assert {Path(first.saved).name, Path(second.saved).name} == {
            "report.pdf",
            "report (1).pdf",
        }
        assert not reserved

    with tempfile.TemporaryDirectory() as tmp:
        asyncio.run(run(tmp))


def test_instance_stop_is_safe_before_and_after_loop_lifecycle():
    inst = w.Instance(make_profile(), False, None)
    inst.request_stop()

    async def run():
        stop_event = inst.bind_loop()
        await asyncio.wait_for(stop_event.wait(), timeout=1)

    asyncio.run(run())
    inst.wake()
    assert inst.stop_requested


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
        assert browser_patches._NEWTAB_BUTTON_MARKER in t1
        assert '#tabbrowser-tabs:not([overflow]) #tabs-newtab-button' in t1
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


def test_chrome_css_patches_create_profile_stylesheet():
    with tempfile.TemporaryDirectory() as tmp:
        exe = Path(tmp) / "camoufox.exe"
        profile = Path(tmp) / "profile"
        browser_patches._ensure_chrome_css_patches(str(exe), str(profile))
        css = profile / "chrome" / "userChrome.css"
        patched = css.read_text(encoding="utf-8")
        assert not (exe.parent / "chrome.css").exists()
        for marker, _ in browser_patches._CHROME_CSS_PATCHES:
            assert patched.count(marker) == 1
        custom = "\n#PersonalToolbar { opacity: 0.9; }\n"
        css.write_text(patched + custom, encoding="utf-8")
        browser_patches._ensure_chrome_css_patches(str(exe), str(profile))
        assert css.read_text(encoding="utf-8") == patched + custom


def test_native_focus_patch_preserves_browser_archive():
    if os.name != "nt":
        return
    import zipfile

    with tempfile.TemporaryDirectory() as tmp:
        exe = Path(tmp) / "camoufox.exe"
        archive = exe.parent / "omni.ja"
        module = "modules/juggler/content/main.js"
        with zipfile.ZipFile(archive, "w") as source:
            source.writestr(module, "docShell.overrideHasFocus = true;\ndocShell.forceActiveState = true;")
            source.writestr("unrelated.bin", b"\x00\xff\x01")
        original = archive.read_bytes()
        browser_patches._ensure_native_focus_patch(str(exe))
        with zipfile.ZipFile(archive) as result:
            assert result.read("unrelated.bin") == b"\x00\xff\x01"
            assert b"focusmanager.testmode" in result.read(module)
            assert b"docShell.forceActiveState = true;" in result.read(module)
        assert (exe.parent / "omni.ja.camouforge-original").read_bytes() == original
        patched = archive.read_bytes()
        browser_patches._ensure_native_focus_patch(str(exe))
        assert archive.read_bytes() == patched


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
