from __future__ import annotations

import os
import re
import shutil
import sys
from functools import lru_cache
from pathlib import Path
from typing import Any, Dict, Optional

MACOS_EMOJI_FAMILY = "Apple Color Emoji"
_MACOS_EMOJI_FONT = "AppleColorEmoji.ttf"
_MACOS_EMOJI_REPAIR_DIR = ".camouforge-font-repair"
_MACOS_EMOJI_SOURCE = "AppleColorEmoji.camouforge-source"

_TITLEBAR_SVG_MARKER = "CAMOUFORGE_TITLEBAR_SVG"
_TITLEBAR_SVG_CSS = """
/* ===== CamouForge patch: SVG window caption buttons =====
 * Fingerprint injection replaces the available fonts with the bundled
 * macOS set (fonts/), so "Segoe Fluent Icons" / "Segoe MDL2 Assets" are
 * missing and browser.css glyphs (\\e921 \\e922 \\e923 \\e8bb) render as hex
 * boxes. Draw the caption buttons with inline SVG instead of system icon
 * fonts. Marker: CAMOUFORGE_TITLEBAR_SVG (worker re-applies if missing).
 */
.titlebar-min,
.titlebar-max,
.titlebar-restore,
.titlebar-close {
  content: "" !important;
  background-position: center !important;
  background-repeat: no-repeat !important;
}
.titlebar-min {
  background-image: url("data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='12'%20height='12'%3E%3Cpath%20d='M1%206h10'%20stroke='white'%20stroke-opacity='.85'/%3E%3C/svg%3E") !important;
}
.titlebar-max {
  background-image: url("data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='12'%20height='12'%3E%3Crect%20x='2'%20y='2'%20width='8'%20height='8'%20fill='none'%20stroke='white'%20stroke-opacity='.85'/%3E%3C/svg%3E") !important;
}
.titlebar-restore {
  background-image: url("data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='12'%20height='12'%3E%3Crect%20x='1.5'%20y='3.5'%20width='7'%20height='7'%20fill='none'%20stroke='white'%20stroke-opacity='.85'/%3E%3Cpath%20d='M3.5%203.5v-2h7v7h-2'%20fill='none'%20stroke='white'%20stroke-opacity='.85'/%3E%3C/svg%3E") !important;
}
.titlebar-close {
  background-image: url("data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='12'%20height='12'%3E%3Cpath%20d='M2%202l8%208M10%202l-8%208'%20stroke='white'%20stroke-opacity='.85'/%3E%3C/svg%3E") !important;
}
"""

_TAB_CLICK_MARKER = "CAMOUFORGE_TAB_CLICK"
_TAB_CLICK_CSS = """
/* CamouForge patch: restore normal single-click tab selection.
 * The bundled theme makes #TabsToolbar draggable and lets tabs inherit that
 * state while disabling their content's pointer events. Keep only the empty
 * toolbar area draggable. Marker: CAMOUFORGE_TAB_CLICK. */
#TabsToolbar .tabbrowser-tab,
#TabsToolbar .tab-content {
  -moz-window-dragging: no-drag !important;
}
#TabsToolbar .tab-content {
  pointer-events: auto !important;
}
"""

_TAB_LAYOUT_MARKER = "CAMOUFORGE_TAB_LAYOUT"
_TAB_LAYOUT_CSS = """
/* Keep regular tabs bounded so the trailing toolbar remains draggable.
 * Marker: CAMOUFORGE_TAB_LAYOUT. */
#TabsToolbar .tabbrowser-tab[fadein]:not([pinned]) {
  max-width: 240px !important;
}
"""

_CHROME_DARK_THEME_MARKER = "CAMOUFORGE_CHROME_DARK_THEME"
_CHROME_DARK_THEME_CSS = """
/* Match Chrome's dark theme color hierarchy: a near-black inactive tab strip,
 * a charcoal active tab/navigation surface, and a dark omnibox that turns
 * light while focused. Marker: CAMOUFORGE_CHROME_DARK_THEME. */
:root {
  --lwt-accent-color: #1f2020 !important;
  --lwt-text-color: #e8eaed !important;
  --toolbar-bgcolor: #3c3c3c !important;
  --toolbar-color: #e8eaed !important;
  --tab-selected-bgcolor: #3c3c3c !important;
  --tab-selected-textcolor: #e8eaed !important;
  --toolbar-field-background-color: #292a2d !important;
  --toolbar-field-color: #e8eaed !important;
  --toolbar-field-focus-background-color: #f1f3f4 !important;
  --toolbar-field-focus-color: #202124 !important;
  --toolbar-field-border-color: transparent !important;
  --toolbar-field-focus-border-color: #8ab4f8 !important;
  --toolbarbutton-icon-fill: #e8eaed !important;
  --chrome-content-separator-color: #3c3c3c !important;
}

#navigator-toolbox,
#TabsToolbar {
  background-color: #1f2020 !important;
  color: #e8eaed !important;
}

#nav-bar {
  background-color: #3c3c3c !important;
  color: #e8eaed !important;
}

.tabbrowser-tab[selected="true"] .tab-background {
  background-color: #3c3c3c !important;
}

.tabbrowser-tab:not([selected="true"]):hover .tab-background {
  background-color: #292a2d !important;
}

#urlbar:not(:focus-within):not([open]) > #urlbar-background {
  background-color: #292a2d !important;
}

#urlbar:focus-within > #urlbar-background,
#urlbar[open] > #urlbar-background {
  background-color: #f1f3f4 !important;
  border-color: #8ab4f8 !important;
}

#urlbar:focus-within,
#urlbar[open] {
  color: #202124 !important;
}

#urlbar:focus-within .urlbar-icon,
#urlbar[open] .urlbar-icon {
  fill: #5f6368 !important;
}
"""

_CHROME_CSS_PATCHES = (
    (_TITLEBAR_SVG_MARKER, _TITLEBAR_SVG_CSS),
    (_TAB_CLICK_MARKER, _TAB_CLICK_CSS),
    (_TAB_LAYOUT_MARKER, _TAB_LAYOUT_CSS),
    (_CHROME_DARK_THEME_MARKER, _CHROME_DARK_THEME_CSS),
)


def _font_has_cbdt_emoji(path: Path) -> bool:
    try:
        from fontTools.ttLib import TTFont

        with TTFont(path, lazy=True) as font:
            return (
                "CBDT" in font
                and "CBLC" in font
                and font["name"].getDebugName(1) == MACOS_EMOJI_FAMILY
            )
    except Exception:
        return False


def _font_has_svg_emoji(path: Path) -> bool:
    try:
        from fontTools.ttLib import TTFont

        with TTFont(path, lazy=True) as font:
            head = font["head"]
            hhea = font["hhea"]
            return (
                "SVG " in font
                and "glyf" in font
                and "CBDT" not in font
                and "CBLC" not in font
                and font["name"].getDebugName(1) == MACOS_EMOJI_FAMILY
                and head.xMax > head.xMin
                and head.yMax > head.yMin
                and hhea.xMaxExtent > 0
                and _svg_images_are_firefox_compatible(font)
            )
    except Exception:
        return False


@lru_cache(maxsize=8)
def _cached_font_has_svg_emoji(path: str, _size: int, _mtime_ns: int) -> bool:
    return _font_has_svg_emoji(Path(path))


def _validated_svg_emoji(path: Path) -> bool:
    try:
        stat = path.stat()
    except OSError:
        return False
    return _cached_font_has_svg_emoji(
        str(path.resolve()), stat.st_size, stat.st_mtime_ns
    )


def _restore_font_bounds(font: Any) -> None:
    head = font["head"]
    hhea = font["hhea"]
    if head.xMax <= head.xMin or head.yMax <= head.yMin:
        head.xMin = 0
        head.yMin = hhea.descent
        head.xMax = hhea.advanceWidthMax
        head.yMax = hhea.ascent
    if hhea.xMaxExtent <= 0:
        hhea.xMaxExtent = hhea.advanceWidthMax


def _svg_images_are_firefox_compatible(font: Any) -> bool:
    documents = font["SVG "].docList
    return bool(documents) and all(
        'xmlns:xlink="http://www.w3.org/1999/xlink"' in document.data
        and 'xlink:href="data:image/png;base64,' in document.data
        for document in documents
    )


def _convert_apple_emoji_to_svg(source: Path, destination: Path) -> None:
    import base64

    from fontTools.pens.ttGlyphPen import TTGlyphPen
    from fontTools.ttLib import TTFont, newTable

    with TTFont(source, recalcBBoxes=False, recalcTimestamp=False) as font:
        if "CBDT" not in font or "CBLC" not in font:
            raise ValueError("缺少 CBDT/CBLC 彩色位图表")

        # Firefox on Windows needs outline tables even when SVG supplies the image.
        glyph_order = font.getGlyphOrder()
        glyf = newTable("glyf")
        glyf.glyphs = {name: TTGlyphPen(None).glyph() for name in glyph_order}
        glyf.glyphOrder = glyph_order
        font["glyf"] = glyf
        font["loca"] = newTable("loca")

        maxp = font["maxp"]
        maxp.tableVersion = 0x00010000
        maxp.maxZones = 1
        for field in (
            "maxPoints",
            "maxContours",
            "maxCompositePoints",
            "maxCompositeContours",
            "maxTwilightPoints",
            "maxStorage",
            "maxFunctionDefs",
            "maxInstructionDefs",
            "maxStackElements",
            "maxSizeOfInstructions",
            "maxComponentElements",
            "maxComponentDepth",
        ):
            setattr(maxp, field, 0)

        strike = font["CBDT"].strikeData[0]
        ppem = font["CBLC"].strikes[0].bitmapSizeTable.ppemY
        scale = font["head"].unitsPerEm / ppem
        svg = newTable("SVG ")
        svg.docList = []
        for glyph_name in sorted(strike, key=font.getGlyphID):
            glyph_id = font.getGlyphID(glyph_name)
            bitmap = strike[glyph_name]
            bitmap.ensureDecompiled()
            metrics = bitmap.metrics
            png = base64.b64encode(bitmap.imageData).decode("ascii")
            # Firefox 152 renders embedded PNGs through SVG 1.1 xlink; plain
            # SVG2 href produces transparent glyphs despite valid metrics.
            document = (
                '<svg version="1.1" xmlns="http://www.w3.org/2000/svg" '
                'xmlns:xlink="http://www.w3.org/1999/xlink">'
                f'<g id="glyph{glyph_id}">'
                f'<image x="{metrics.BearingX * scale:.3f}" '
                f'y="{-metrics.BearingY * scale:.3f}" '
                f'width="{metrics.width * scale:.3f}" '
                f'height="{metrics.height * scale:.3f}" '
                f'xlink:href="data:image/png;base64,{png}"/>'
                "</g></svg>"
            )
            svg.docList.append((document, glyph_id, glyph_id, True))
        font["SVG "] = svg
        del font["CBDT"]
        del font["CBLC"]
        _restore_font_bounds(font)
        font.save(destination, reorderTables=False)


def _backup_emoji_source(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    pending = destination.with_name(f"{destination.name}.pending")
    pending.unlink(missing_ok=True)
    shutil.copyfile(source, pending)
    if not _font_has_cbdt_emoji(pending):
        pending.unlink(missing_ok=True)
        raise RuntimeError("原字体备份校验失败")
    os.replace(pending, destination)


def _ensure_macos_emoji_font(executable_path: Optional[str]) -> None:
    """让 Windows Camoufox 可绘制捆绑的原始 Apple emoji 字形。"""
    if os.name != "nt" or not executable_path:
        return
    browser_dir = Path(executable_path).parent
    fonts_dir = browser_dir / "fonts"
    destination = fonts_dir / _MACOS_EMOJI_FONT
    pending = destination.with_name(f"{destination.name}.pending")
    source = browser_dir / _MACOS_EMOJI_REPAIR_DIR / _MACOS_EMOJI_SOURCE
    try:
        if _validated_svg_emoji(destination):
            pending.unlink(missing_ok=True)
            return
        if not destination.exists():
            print(f"[camoforge] Apple emoji 修复跳过：{destination} 不存在", file=sys.stderr)
            return

        if _validated_svg_emoji(pending):
            os.replace(pending, destination)
            print(f"[camoforge] 已启用 Windows 兼容的 Apple Color Emoji: {destination}", file=sys.stderr)
            return

        if _font_has_cbdt_emoji(destination):
            # Keep the source outside fonts/: Camoufox scans bundled font data
            # regardless of extension, and a second Apple family shadows SVG.
            _backup_emoji_source(destination, source)
        elif not _font_has_cbdt_emoji(source):
            raise RuntimeError("找不到可转换的 Apple Color Emoji 原字体")

        pending.unlink(missing_ok=True)
        _convert_apple_emoji_to_svg(source, pending)
        if not _validated_svg_emoji(pending):
            raise RuntimeError("转换结果校验失败")
        os.replace(pending, destination)
        print(f"[camoforge] 已启用 Windows 兼容的 Apple Color Emoji: {destination}", file=sys.stderr)
    except Exception as error:
        print(
            f"[camoforge] Apple emoji 修复暂未应用（关闭已运行的浏览器后重试）: "
            f"{destination}: {error}",
            file=sys.stderr,
        )


def _ensure_macos_emoji_whitelist() -> None:
    """把 Apple Color Emoji 固定为 macOS 指纹的必备字体。"""
    try:
        import camoufox.fingerprints as fingerprints

        fonts = fingerprints._ESSENTIAL_FONTS_MACOS
        if MACOS_EMOJI_FAMILY not in fonts:
            fonts.append(MACOS_EMOJI_FAMILY)
    except Exception as error:
        print(f"[camoforge] Apple emoji 字体白名单修复失败: {error}", file=sys.stderr)

# `__TILES__` 占位符由 _shortcut_page 用真实条目替换；不含任何外部资源请求。
_SHORTCUTS_PAGE_TEMPLATE = """<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>快捷方式</title>
<style>
  :root { color-scheme: dark; }
  html, body { height: 100%; }
  body {
    margin: 0; font-family: -apple-system, "Segoe UI", Roboto, "PingFang SC",
      "Microsoft YaHei", sans-serif; background: #3c3c3c; color: #e8eaed;
    display: flex; justify-content: center; align-items: flex-start;
  }
  .wrap { width: 100%; max-width: 960px; padding: 48px 32px; box-sizing: border-box; }
  h1 { font-size: 20px; font-weight: 600; margin: 0 0 28px; color: #f1f3f4; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(96px, 1fr)); gap: 20px; }
  .tile { display: flex; flex-direction: column; align-items: center; gap: 10px;
          text-decoration: none; color: #e8eaed; padding: 14px 8px; border-radius: 12px; }
  .tile:hover { background: #4a4a4a; }
  .icon { width: 48px; height: 48px; border-radius: 50%; display: flex; align-items: center;
          justify-content: center; font-size: 22px; font-weight: 600; color: #fff; }
  .name { font-size: 13px; text-align: center; max-width: 96px; overflow: hidden;
          text-overflow: ellipsis; white-space: nowrap; }
</style>
</head>
<body>
<div class="wrap">
  <h1>快捷方式</h1>
  <div class="grid">
__TILES__
  </div>
</div>
</body>
</html>
"""


def _html_escape(text: str) -> str:
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
        .replace("'", "&#39;")
    )


def _build_shortcuts_html(shortcuts: list) -> str:
    tiles = []
    for i, s in enumerate(shortcuts):
        if not isinstance(s, dict):
            continue
        url = str(s.get("url") or "").strip()
        if not url:
            continue
        if not url.startswith(("http://", "https://")):
            url = "https://" + url
        name = str(s.get("name") or "").strip()
        display = name or url
        letter = display[0].upper() if display else "?"
        hue = (i * 47) % 360
        tiles.append(
            f'<a class="tile" href="{_html_escape(url)}">'
            f'<span class="icon" style="background:hsl({hue} 60% 38%)">'
            f"{_html_escape(letter)}</span>"
            f'<span class="name" title="{_html_escape(display)}">'
            f"{_html_escape(display)}</span></a>"
        )
    return _SHORTCUTS_PAGE_TEMPLATE.replace("__TILES__", "\n".join(tiles))


def _shortcuts_list(profile: Dict[str, Any]) -> list:
    return (profile.get("launch") or {}).get("shortcuts") or []


def _data_dir() -> Path:
    return Path(
        os.environ.get("CAMOFORGE_DATA_DIR", str(Path.home() / ".camoforge"))
    )


def _shortcut_page(profile: Dict[str, Any]) -> Optional[str]:
    """把 profile 的 launch.shortcuts 渲染成快捷方式首页 HTML，返回 file:// URL。

    无快捷方式返回 None；写入失败返回 None（不影响启动，走默认空白页）。
    """
    shortcuts = _shortcuts_list(profile)
    if not shortcuts:
        return None
    out_dir = _data_dir() / "shortcuts"
    try:
        out_dir.mkdir(parents=True, exist_ok=True)
    except Exception:
        return None
    html = _build_shortcuts_html(shortcuts)
    path = out_dir / f"{profile.get('id', 'default')}.html"
    try:
        path.write_text(html, encoding="utf-8")
    except Exception:
        return None
    return path.as_uri()


# 新标签页覆盖扩展的 manifest（chrome_url_overrides.newtab 是 Firefox 覆盖
# about:newtab 的标准机制；camoufox 以 profile/application scope 加载扩展并自动启用）。
_NEWTAB_EXT_MANIFEST = """{
  "manifest_version": 2,
  "name": "CamouForge New Tab",
  "version": "1.0",
  "description": "Show configured shortcuts on every new tab.",
  "chrome_url_overrides": { "newtab": "newtab.html" }
}
"""


def _shortcut_extension_dir(profile: Dict[str, Any]) -> Optional[str]:
    """生成覆盖 about:newtab 的小型扩展目录（manifest + newtab.html），返回路径。

    该目录会被追加进 launch 的 addons，使每个新标签页（Ctrl+T）也显示快捷方式网格，
    与 Chrome 行为一致。无快捷方式或写入失败返回 None。
    """
    shortcuts = _shortcuts_list(profile)
    if not shortcuts:
        return None
    ext_dir = _data_dir() / "shortcuts_ext" / str(profile.get("id", "default"))
    try:
        ext_dir.mkdir(parents=True, exist_ok=True)
        (ext_dir / "manifest.json").write_text(_NEWTAB_EXT_MANIFEST, encoding="utf-8")
        (ext_dir / "newtab.html").write_text(
            _build_shortcuts_html(shortcuts), encoding="utf-8"
        )
    except Exception:
        return None
    return str(ext_dir)


def _ensure_chrome_css_patches(executable_path: Optional[str]) -> None:
    """幂等补齐 Camoufox 浏览器 chrome.css 修复。"""
    if not executable_path:
        return
    css = Path(executable_path).parent / "chrome.css"
    try:
        if not css.exists():
            print(f"[camoforge] 浏览器样式补丁跳过：{css} 不存在", file=sys.stderr)
            return
        text = css.read_text(encoding="utf-8", errors="replace")
        missing = [body for marker, body in _CHROME_CSS_PATCHES if marker not in text]
        if not missing:
            return
        css.write_text(text + "".join(missing), encoding="utf-8")
        written = css.read_text(encoding="utf-8", errors="replace")
        failed = [marker for marker, _ in _CHROME_CSS_PATCHES if marker not in written]
        if failed:
            print(f"[camoforge] 浏览器样式补丁写入后校验失败: {css}: {failed}", file=sys.stderr)
    except Exception as e:
        print(f"[camoforge] 浏览器样式补丁失败: {css}: {e}", file=sys.stderr)


_SESSION_HISTORY_MARKER = "CAMOUFORGE_SESSION_HISTORY_50"
# camoufox 出厂把 browser.sessionhistory.max_entries 钉成 0：真实浏览器不可能为 0
# （可被读取识破），且 history.back()/go() 变静默空操作——网页内用历史路由关的
# 弹窗（如 ChatGPT 设置）从此关不掉。nsSHistory 在 profile prefs 应用前就读了
# default 值，所以必须改 camoufox.cfg 的 defaultPref，user_pref 覆盖无效。
_SESSION_HISTORY_GOOD = 'defaultPref("browser.sessionhistory.max_entries", 50); // ' + _SESSION_HISTORY_MARKER


def _ensure_session_history_pref(executable_path: Optional[str]) -> None:
    if not executable_path:
        return
    cfg = Path(executable_path).parent / "camoufox.cfg"
    try:
        if not cfg.exists():
            print(f"[camoforge] 会话历史修复跳过：{cfg} 不存在", file=sys.stderr)
            return
        text = cfg.read_text(encoding="utf-8", errors="replace")
        m = re.search(r'defaultPref\("browser\.sessionhistory\.max_entries",\s*\d+\)[^\r\n]*', text)
        if m:
            if m.group(0).rstrip().endswith("50);"):
                return
            text = text[:m.start()] + _SESSION_HISTORY_GOOD + text[m.end():]
        else:
            print(f"[camoforge] 会话历史修复跳过：未找到出厂行（可能上游已改）", file=sys.stderr)
            return
        cfg.write_text(text, encoding="utf-8")
        check = cfg.read_text(encoding="utf-8", errors="replace")
        m2 = re.search(r'defaultPref\("browser\.sessionhistory\.max_entries",\s*(\d+)\)', check)
        if not m2 or m2.group(1) != "50":
            print(f"[camoforge] 会话历史修复写入后校验失败: {cfg}", file=sys.stderr)
        else:
            print(f"[camoforge] 已修复 browser.sessionhistory.max_entries: {cfg}", file=sys.stderr)
    except Exception as e:
        print(f"[camoforge] 会话历史修复失败: {cfg}: {e}", file=sys.stderr)
