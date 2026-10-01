from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any, Dict

from browser_patches import MACOS_DEFAULT_FONTS, MACOS_EMOJI_FAMILY
from sdk_bridge import (
    SDK,
    VALID_OS,
    _apply_window_to_fingerprint,
    _default_executable_path,
    _infer_ff_version,
    _rebuild_fingerprint,
)

_UBO = "ubo"


def _fingerprint_is_macos(data: Dict[str, Any]) -> bool:
    navigator = data.get("navigator") or {}
    headers = data.get("headers") or {}
    user_agent = navigator.get("userAgent") or headers.get("User-Agent") or ""
    return "Macintosh" in user_agent or "Mac OS X" in user_agent


def _with_macos_emoji(fonts: list) -> list:
    result = list(fonts)
    if MACOS_EMOJI_FAMILY not in result:
        result.append(MACOS_EMOJI_FAMILY)
    return result


def translate_profile(profile: Dict[str, Any], user_data_root: Path) -> Dict[str, Any]:
    """把 CamouForge Profile JSON 转成 Camoufox(**kwargs) 参数。

    LaunchOptions 的语义枚举在此展开为 SDK 原生参数；None/未设置 = SDK 默认。
    """
    lo: Dict[str, Any] = profile.get("launch") or {}
    config: Dict[str, Any] = dict(profile.get("config") or {})
    kw: Dict[str, Any] = {"config": config}

    os_list = lo.get("os")
    if os_list:
        bad = [o for o in os_list if o not in VALID_OS]
        if bad:
            raise ValueError(f"invalid OS values: {bad} (expected any of {VALID_OS})")
        kw["os"] = os_list[0] if len(os_list) == 1 else os_list
    macos_target = os_list == ["macos"] or _fingerprint_is_macos(
        lo.get("fingerprint") or {}
    )

    hm = lo.get("humanize")
    if hm and hm.get("mode") != "off":
        if hm["mode"] == "custom":
            kw["humanize"] = float(hm["max_time"])
        else:
            kw["humanize"] = True

    hl = lo.get("headless") or "headed"
    if hl == "headless":
        kw["headless"] = True
    elif hl == "virtual":
        kw["headless"] = "virtual"

    proxy = lo.get("proxy") or {}
    if proxy.get("server"):
        p = {"server": proxy["server"]}
        if proxy.get("username"):
            p["username"] = proxy["username"]
        if proxy.get("password"):
            p["password"] = proxy["password"]
        kw["proxy"] = p
        if proxy.get("port") is not None:
            config["port"] = int(proxy["port"])

    geo = lo.get("geoip")
    if geo and geo.get("mode") != "off":
        if geo["mode"] == "ip":
            kw["geoip"] = geo["ip"]
        else:
            kw["geoip"] = True
        if lo.get("geoip_db"):
            kw["geoip_db"] = lo["geoip_db"]

    if lo.get("fingerprint"):
        fp_obj = _rebuild_fingerprint(lo["fingerprint"])
        kw["fingerprint"] = fp_obj
        # 指纹已携带 navigator/screen/window/UA 等信息；config 里同域键属于重复的
        # 「手动配置」，既触发 navigator/viewport 泄漏告警，又会与指纹冲突，移除。
        dup = ("navigator.", "window.", "screen.", "headers.User-Agent", "locale:")
        config = {k: v for k, v in config.items() if not k.startswith(dup)}
        # 指纹一致性：camoufox 的 browserforge.yml 故意不把 screen.devicePixelRatio 映射成
        # window.devicePixelRatio（"Any value other than 1.0 is suspicious"），导致
        # window.devicePixelRatio 泄漏真实 DPR（4K 屏 150% 缩放 = 1.5），与指纹 screen
        # （如 3440x1440 应 DPR=1）自相矛盾。这里显式把指纹 DPR 写回 config 补上。
        dpr = getattr(getattr(fp_obj, "screen", None), "devicePixelRatio", None)
        if dpr:
            config["window.devicePixelRatio"] = dpr
        kw["config"] = config
    fp_preset = lo.get("fingerprint_preset")
    if fp_preset and fp_preset.get("mode") != "off":
        if fp_preset["mode"] == "random":
            kw["fingerprint_preset"] = True
        elif fp_preset["mode"] == "value":
            kw["fingerprint_preset"] = fp_preset["value"]

    for src in ("locale", "ff_version", "executable_path", "browser"):
        v = lo.get(src)
        if v is not None and v != "" and not (src == "ff_version" and v == 0):
            kw[src] = v

    if isinstance(kw.get("locale"), list):
        loc = kw["locale"]
        kw["locale"] = loc[0] if len(loc) == 1 else loc

    config_fonts = config.get("fonts")
    if macos_target and isinstance(config_fonts, list) and config_fonts:
        config["fonts"] = _with_macos_emoji(config_fonts)
    profile_fonts = lo.get("fonts")
    if profile_fonts:
        kw["fonts"] = (
            _with_macos_emoji(profile_fonts)
            if macos_target
            else list(profile_fonts)
        )
    elif macos_target and not config_fonts and not (
        fp_preset and fp_preset.get("mode") != "off"
    ):
        from camoufox.fingerprints import _generate_random_font_subset

        kw["fonts"] = _with_macos_emoji(_generate_random_font_subset("macos"))
        for families in MACOS_DEFAULT_FONTS.values():
            for family in families:
                if family not in kw["fonts"]:
                    kw["fonts"].append(family)
    if lo.get("custom_fonts_only") is not None:
        kw["custom_fonts_only"] = lo["custom_fonts_only"]

    screen = lo.get("screen")
    if screen and screen.get("mode") != "off":
        from browserforge.fingerprints import Screen
        if screen["mode"] == "exact":
            kw["screen"] = Screen(min_width=screen["width"], max_width=screen["width"],
                                  min_height=screen["height"], max_height=screen["height"])
        else:  # range
            kw["screen"] = Screen(min_width=screen["min_width"], max_width=screen["max_width"],
                                  min_height=screen["min_height"], max_height=screen["max_height"])
    # 传了 fingerprint 时 SDK 会忽略 window 参数（窗口尺寸由指纹 screen 决定），
    # 所以要把目标尺寸直接写进指纹；无指纹时才用 window 参数走 SDK 随机生成。
    fp = kw.get("fingerprint")
    if lo.get("window"):
        w = (int(lo["window"][0]), int(lo["window"][1]))
        if fp is not None:
            _apply_window_to_fingerprint(fp, w)
        else:
            kw["window"] = w
    elif hl == "headed":
        # camoufox 未指定 window 时会随机生成贴近指纹屏幕的尺寸（几乎全屏），体感过大。
        # 默认 1280x720：≤ 最小常见指纹屏幕 1366x768 的可用区，避免 outer > screen 的
        # 不可能几何泄漏；profile 的 launch.window 可随时覆盖。
        if fp is not None:
            _apply_window_to_fingerprint(fp, (1280, 720))
        else:
            kw["window"] = (1280, 720)

    wc = lo.get("webgl_config")
    if wc and wc.get("vendor"):
        kw["webgl_config"] = (wc["vendor"], wc.get("renderer") or "")

    for k in ("block_images", "block_webrtc", "block_webgl", "disable_coop",
              "main_world_eval", "enable_cache", "debug", "i_know_what_im_doing"):
        v = lo.get(k)
        if v is not None:
            kw[k] = v

    if lo.get("addons"):
        kw["addons"] = lo["addons"]
    if lo.get("exclude_addons"):
        enum_cls = SDK.default_addons.DefaultAddons
        kw["exclude_addons"] = [enum_cls.UBO if a == _UBO else enum_cls(a) for a in lo["exclude_addons"]]

    if lo.get("firefox_user_prefs"):
        kw["firefox_user_prefs"] = lo["firefox_user_prefs"]
    if lo.get("args"):
        kw["args"] = lo["args"]
    if lo.get("env"):
        kw["env"] = lo["env"]
    if lo.get("extra_launch_options"):
        kw.update(lo["extra_launch_options"])

    # Playwright Firefox 驱动在未显式指定时默认把 context colorScheme 设成
    # "light"（doUpdateDefaultEmulatedMedia 的默认值写死 "light"），导致站点
    # 看到的 prefers-color-scheme 恒为亮色：浏览器 chrome 是暗色，支持暗色的
    # 站点（如 ippure.com）也会被渲染成白色。profile 显式设 "dark"/"light"
    # 时透传给驱动；未设置则保持驱动默认（不改变现有行为）。
    cs = lo.get("color_scheme")
    if cs in ("dark", "light"):
        kw["color_scheme"] = cs

    if lo.get("persistent_context"):
        kw["persistent_context"] = True
        udd = lo.get("user_data_dir")
        if not udd:
            udd = str(user_data_root / profile["id"])
        kw["user_data_dir"] = udd

    if not kw.get("executable_path"):
        default_exe = _default_executable_path()
        if default_exe:
            kw["executable_path"] = default_exe

    # 自动推断 ff_version：自定义 executable_path 但未指定 ff_version 时，
    # camoufox 会查默认安装目录并抛 CamoufoxNotInstalled。
    if kw.get("executable_path") and not kw.get("ff_version"):
        inferred = _infer_ff_version(kw["executable_path"])
        if inferred:
            kw["ff_version"] = inferred

    # 应用托管的指纹 / ff_version 属有意设置（指纹由 camoufox 自身生成、ff_version
    # 与本地浏览器一致），默认确认以抑制 custom_fingerprint / ff_version 告警；
    # 用户在 profile 显式设 i_know_what_im_doing=false 可恢复全部告警。
    if kw.get("i_know_what_im_doing") is None and (
        kw.get("fingerprint") is not None or kw.get("ff_version")
    ):
        kw["i_know_what_im_doing"] = True

    # 下载：Juggler 版 Firefox 在 accept_downloads 未显式开启时会拦截下载却无处落盘
    # （静默丢弃）。必须显式开启。accept_downloads 是「上下文」级参数：
    # - 持久化上下文：随 launch_persistent_context 传入（放进 kw 即生效）；
    # - 非持久化：playwright.firefox.launch() 不收该参数（会抛 TypeError），改在
    #   Manager.run 里对 browser.new_context(accept_downloads=True) 显式开启。
    if kw.get("persistent_context"):
        kw["accept_downloads"] = True

    prefs = dict(kw.get("firefox_user_prefs") or {})
    kw["firefox_user_prefs"] = prefs
    if macos_target:
        for script, families in MACOS_DEFAULT_FONTS.items():
            for style, family in zip(("sans-serif", "serif", "monospace"), families):
                prefs.setdefault(f"font.name.{style}.{script}", family)
                prefs.setdefault(f"font.name-list.{style}.{script}", family)

    navigator = (lo.get("fingerprint") or {}).get("navigator") or {}
    locales = kw.get("locale") or navigator.get("languages") or navigator.get("language")
    if locales:
        if isinstance(locales, str):
            locales = [locale.strip() for locale in locales.split(",")]
        prefs.setdefault("intl.locale.requested", ",".join(locales))

    # 156 reads startup-only prefs before Juggler applies non-persistent prefs.
    if prefs:
        kw["env"] = {**os.environ, **(kw.get("env") or {})}
        kw["env"]["CAMOU_PREFS_1"] = json.dumps(prefs)

    return kw

def validate(params: Dict[str, Any]) -> Dict[str, Any]:
    """复用 launch_options() 干跑：收集 LeakWarning 且不启动浏览器。"""
    SDK.load()
    from camoufox.utils import launch_options
    from camoufox._warnings import LeakWarning

    profile = params["profile"]
    collected: list = []

    def fake_warn(warning_key, i_know_what_im_doing=None):
        # 与真实 LeakWarning.warn 一致：i_know_what_im_doing=True 时不告警
        if i_know_what_im_doing:
            return
        try:
            msg = SDK.warnings.WARNINGS_DATA[warning_key]
        except Exception:
            msg = warning_key
        collected.append({"code": warning_key, "message": msg})

    original = LeakWarning.warn
    LeakWarning.warn = staticmethod(fake_warn)
    try:
        kw = translate_profile(profile, Path("/tmp"))
        # 干跑：geoip 会发网络请求，跳过
        kw.pop("geoip", None)
        kw["headless"] = True
        launch_options(**kw)
    except Exception as e:
        return {"warnings": collected, "error": f"{type(e).__name__}: {e}"}
    finally:
        LeakWarning.warn = original
    return {"warnings": collected}
