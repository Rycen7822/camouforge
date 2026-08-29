from __future__ import annotations

import os
import re
import sys
from pathlib import Path
from typing import Any, Dict, Optional

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
      "Microsoft YaHei", sans-serif; background: #0f1115; color: #e6e6e6;
    display: flex; justify-content: center; align-items: flex-start;
  }
  .wrap { width: 100%; max-width: 960px; padding: 48px 32px; box-sizing: border-box; }
  h1 { font-size: 20px; font-weight: 600; margin: 0 0 28px; color: #f5f5f5; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(96px, 1fr)); gap: 20px; }
  .tile { display: flex; flex-direction: column; align-items: center; gap: 10px;
          text-decoration: none; color: #e6e6e6; padding: 14px 8px; border-radius: 12px; }
  .tile:hover { background: #1a1d24; }
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


def _ensure_titlebar_svg_patch(executable_path: Optional[str]) -> None:
    """浏览器目录 chrome.css 缺窗口按钮 SVG 补丁时幂等追加。

    camoufox 的 xul.dll 整体加载浏览器目录的 chrome.css；浏览器被重新解压后
    手工补丁会丢，这里在每次启动前按 marker 自愈。补丁失败不影响启动，但必须
    在日志留痕（stderr → 当日日志文件），否则窗口按钮变方框时无从排查。
    """
    if not executable_path:
        return
    css = Path(executable_path).parent / "chrome.css"
    try:
        if not css.exists():
            print(f"[camoforge] 窗口按钮补丁跳过：{css} 不存在", file=sys.stderr)
            return
        text = css.read_text(encoding="utf-8", errors="replace")
        if _TITLEBAR_SVG_MARKER in text:
            return
        css.write_text(text + _TITLEBAR_SVG_CSS, encoding="utf-8")
        # 写后读回验证：写入被截断/拦截时下一次启动无法自愈（marker 已在）
        if _TITLEBAR_SVG_MARKER not in css.read_text(encoding="utf-8", errors="replace"):
            print(f"[camoforge] 窗口按钮补丁写入后校验失败: {css}", file=sys.stderr)
    except Exception as e:
        print(f"[camoforge] 窗口按钮补丁失败: {css}: {e}", file=sys.stderr)


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
