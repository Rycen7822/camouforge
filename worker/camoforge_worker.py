#!/usr/bin/env python3
"""CamouForge stdio JSON-RPC worker."""

from __future__ import annotations

import json
import os
import sys
import threading
import traceback
from pathlib import Path
from typing import Any, Dict

WORKER_DIR = Path(__file__).resolve().parent
if str(WORKER_DIR) not in sys.path:
    sys.path.insert(0, str(WORKER_DIR))

import sdk_bridge as sdk_bridge
from browser_patches import _ensure_session_history_pref
from downloads import _save_download
from manager import Instance, Manager
from profile_translate import translate_profile, validate
from sdk_bridge import (
    SDK,
    _default_executable_path,
    _infer_ff_version,
    _preload_sdk,
    generate_fingerprint,
    list_fonts,
    list_versions,
    list_voices,
    list_webgl,
)
from win_process import _pid_alive

if os.name == "nt":
    from win_process import _find_browser_pid, _process_snapshot, _tree_pids

WORKER_VERSION = "1"
# 响应与 instance_exited 会跨线程写 stdout，必须保证每行 JSON 原子输出。
_SEND_LOCK = threading.Lock()


def send(obj: Dict[str, Any]) -> None:
    line = json.dumps(obj, ensure_ascii=False) + "\n"
    with _SEND_LOCK:
        sys.stdout.write(line)
        sys.stdout.flush()


def _ping(_params: Dict[str, Any]) -> Dict[str, Any]:
    SDK.load()
    browser_version = None
    try:
        from camoufox.utils import installed_verstr

        browser_version = installed_verstr()
    except Exception:
        pass
    package_version = None
    try:
        from importlib.metadata import version

        package_version = version("camoufox")
    except Exception:
        pass
    return {
        "worker": WORKER_VERSION,
        "camoufox": package_version,
        "browser": browser_version,
    }


def main() -> None:
    manager = Manager(lambda event, data: send({"event": event, "data": data}))
    handlers = {
        "ping": _ping,
        "stop": lambda params: manager.stop(params["profile_id"]),
        "stop_all": lambda _params: manager.stop_all(),
        "list": lambda _params: manager.list(),
        "generate_fingerprint": generate_fingerprint,
        "validate": validate,
        "list_webgl": list_webgl,
        "list_versions": list_versions,
        "list_fonts": list_fonts,
        "list_voices": list_voices,
    }

    send({"event": "ready", "data": {"version": WORKER_VERSION}})
    threading.Thread(target=_preload_sdk, name="camoforge-sdk-preload", daemon=True).start()
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as error:
            send({"id": "?", "ok": False, "error": f"bad json: {error}"})
            continue

        request_id = req.get("id", "?")
        method = req.get("method", "")
        params = req.get("params") or {}
        try:
            if method == "launch":
                result = manager.launch(params["profile"])
                send({"id": request_id, "ok": True, "result": result})
                manager.launch_done(params["profile"]["id"])
                continue
            if method == "shutdown":
                manager.stop_all()
                send({"id": request_id, "ok": True, "result": {"bye": True}})
                return

            handler = handlers.get(method)
            if handler is None:
                send({"id": request_id, "ok": False, "error": f"unknown method: {method}"})
                continue
            send({"id": request_id, "ok": True, "result": handler(params)})
        except Exception as error:
            traceback.print_exc(file=sys.stderr)
            send(
                {
                    "id": request_id,
                    "ok": False,
                    "error": f"{type(error).__name__}: {error}",
                }
            )


if __name__ == "__main__":
    main()
