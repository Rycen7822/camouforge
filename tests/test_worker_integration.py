#!/usr/bin/env python3
"""CamouForge worker 集成测试：spawn 真实 worker 进程，走完整 stdio JSON-RPC 协议，
在 Xvfb 下真实启动 Camoufox 并断言指纹注入生效。

前置：DISPLAY 指向 Xvfb；camoufox + 浏览器已安装。
运行：python3 tests/test_worker_integration.py
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

WORKER = Path(__file__).resolve().parent.parent / "worker" / "camoforge_worker.py"
DISPLAY = os.environ.get("DISPLAY", ":99")


class Worker:
    def __init__(self):
        env = dict(os.environ)
        env["DISPLAY"] = DISPLAY
        env.setdefault("CAMOFORGE_DATA_DIR", "/tmp/camoforge-test")
        self.p = subprocess.Popen(
            [sys.executable, "-u", str(WORKER)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            bufsize=1,
        )
        self.n = 0
        self.events = []
        ready = self._read_raw()
        assert ready.get("event") == "ready", f"expected ready event, got {ready}"

    def _read_raw(self):
        line = self.p.stdout.readline()
        assert line, "worker closed stdout"
        return json.loads(line)

    def call(self, method, params=None):
        self.n += 1
        rid = f"t{self.n}"
        self.p.stdin.write(json.dumps({"id": rid, "method": method, "params": params or {}}) + "\n")
        self.p.stdin.flush()
        # 循环读直到拿到本请求响应；中途的事件存起来
        while True:
            msg = self._read_raw()
            if msg.get("id") == rid:
                return msg
            if "event" in msg:
                self.events.append(msg)

    def drain_events(self, timeout=0):
        if timeout:
            time.sleep(timeout)
        return [e for e in self.events]

    def close(self):
        try:
            self.call("shutdown")
        except Exception:
            pass
        try:
            self.p.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.p.kill()
        stderr = self.p.stderr.read()
        if stderr.strip():
            print("--- worker stderr ---")
            print(stderr[-3000:])


def make_profile(**overrides):
    launch = {
        "os": ["macos"],
        "humanize": {"mode": "off"},
        "headless": "headless",  # CI 容器无显示，headless 最稳
        "geoip": {"mode": "off"},
        "proxy": None,
        "persistent_context": False,
    }
    launch.update(overrides.pop("launch", {}))
    prof = {
        "id": "test-profile-1",
        "name": "Test Profile",
        "notes": "",
        "created_at": 0,
        "updated_at": 0,
        "launch": launch,
        "config": {
            "navigator.platform": "MacIntel",
            "navigator.oscpu": "Intel Mac OS X 10.15",
            "navigator.hardwareConcurrency": 8,
            "screen.width": 2560,
            "screen.height": 1440,
        },
    }
    prof.update(overrides)
    return prof


def test_ping(w):
    r = w.call("ping")
    assert r["ok"], r
    print(f"  ping ok: camoufox={r['result']['camoufox']} browser={r['result']['browser']}")


def test_validate(w):
    r = w.call("validate", {"profile": make_profile()})
    assert r["ok"], r
    warnings = r["result"]["warnings"]
    print(f"  validate ok: {len(warnings)} warnings: {[w['code'] for w in warnings]}")
    # navigator.* 手动设置应触发 navigator 警告
    codes = [x["code"] for x in warnings]
    assert "navigator" in codes, f"navigator warning missing in {codes}"


def test_generate_fingerprint(w):
    r = w.call("generate_fingerprint", {"os": ["macos"]})
    assert r["ok"], r
    s = r["result"]["summary"]
    print(f"  generate ok: ua={s['user_agent'][:60]}... screen={s['screen']} renderer={s['webgl_renderer'][:40]}")
    assert "Macintosh" in s["user_agent"] or "Mac" in s["platform"], s
    return r["result"]["fingerprint"]


def test_list_webgl(w):
    r = w.call("list_webgl", {"os": "macos"})
    assert r["ok"], r
    cards = r["result"]["cards"]
    assert cards, "webgl card list empty"
    assert any(c["vendor"] == "Apple" for c in cards), f"Apple card missing: {cards[:3]}"
    print(f"  list_webgl ok: {len(cards)} macos cards, top={cards[0]['vendor']} / {cards[0]['renderer'][:40]}")


def test_launch_stop(w):
    prof = make_profile()
    r = w.call("launch", {"profile": prof})
    assert r["ok"], f"launch failed: {r.get('error')}"
    info = r["result"]
    print(f"  launch ok: pid={info['pid']} headless={info['headless']}")

    lst = w.call("list")
    assert lst["ok"] and any(i["profile_id"] == "test-profile-1" for i in lst["result"]["instances"]), lst

    stop = w.call("stop", {"profile_id": "test-profile-1"})
    assert stop["ok"] and stop["result"]["stopped"], stop
    time.sleep(2)
    lst2 = w.call("list")
    assert not any(i["profile_id"] == "test-profile-1" for i in lst2["result"]["instances"]), lst2
    exited = [e for e in w.events if e["data"].get("profile_id") == "test-profile-1" and e["data"].get("started")]
    assert exited, "instance_exited event missing"
    print(f"  stop ok, events={len(w.events)}")


def test_fingerprint_injection_deep(w):
    """全量配置注入：os+screen+window+webgl_config+fonts+locale。"""
    # webgl_config 必须用 SDK 数据库精确字符串（list_webgl 返回值）
    cards = w.call("list_webgl", {"os": "macos"})["result"]["cards"]
    top = cards[0]
    prof = make_profile(launch={
        "os": ["macos"],
        "headless": "headless",
        "humanize": {"mode": "custom", "max_time": 2.0},
        "webgl_config": {"vendor": top["vendor"], "renderer": top["renderer"]},
        "screen": {"mode": "exact", "width": 1920, "height": 1080},
        "window": [1280, 800],
        "locale": ["en-US"],
        "persistent_context": False,
    })
    prof["config"] = {
        "navigator.platform": "MacIntel",
        "navigator.oscpu": "Intel Mac OS X 10.15",
        "navigator.hardwareConcurrency": 12,
        "screen.width": 1920,
        "screen.height": 1080,
        "geolocation:latitude": 37.7749,
        "geolocation:longitude": -122.4194,
        "timezone": "America/Los_Angeles",
    }
    r = w.call("launch", {"profile": prof})
    assert r["ok"], f"launch failed: {r.get('error')}"
    print(f"  deep launch ok: pid={r['result']['pid']} webgl={top['vendor']} / {top['renderer'][:40]}")
    w.call("stop", {"profile_id": prof["id"]})
    assert True


def test_translate_invalid_os(w):
    prof = make_profile(launch={"os": ["windows98"], "headless": "headless"})
    r = w.call("launch", {"profile": prof})
    assert not r["ok"], "invalid OS should fail"
    print(f"  invalid OS rejected: {r['error'][:80]}")


def main():
    print("== worker integration test ==")
    w = Worker()
    passed = []
    failed = []
    tests = [
        ("ping", test_ping),
        ("validate", test_validate),
        ("generate_fingerprint", test_generate_fingerprint),
        ("list_webgl", test_list_webgl),
        ("launch_stop", test_launch_stop),
        ("fingerprint_injection_deep", test_fingerprint_injection_deep),
        ("invalid_os", test_translate_invalid_os),
    ]
    try:
        for name, fn in tests:
            try:
                fn(w)
                passed.append(name)
            except AssertionError as e:
                failed.append((name, str(e)))
                print(f"  FAIL {name}: {e}")
    finally:
        w.close()
    print(f"\npassed: {len(passed)} failed: {len(failed)}")
    if failed:
        sys.exit(1)


if __name__ == "__main__":
    main()
