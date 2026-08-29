"""CamouForge worker 协议冒烟验证（Windows 侧）。

用 .venv 的 Python 启动 worker，依次发 ping / generate_fingerprint / list_webgl，
验证 camoufox + browserforge 依赖能正常加载并返回数据。
"""
import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
WORKER = ROOT / "worker" / "camoforge_worker.py"
PYTHON = sys.executable

proc = subprocess.Popen(
    [PYTHON, "-u", WORKER],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    text=True,
    bufsize=1,
    encoding="utf-8",
)


def call(method, params=None, rid="1", timeout=180):
    proc.stdin.write(json.dumps({"id": rid, "method": method, "params": params}) + "\n")
    proc.stdin.flush()
    deadline = time.time() + timeout
    while time.time() < deadline:
        line = proc.stdout.readline()
        if not line:
            break
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except Exception:
            print("  [非JSON]", line[:120])
            continue
        if msg.get("id") == rid:
            return msg
        print("  [事件]", json.dumps(msg, ensure_ascii=False)[:160])
    return {"id": rid, "ok": False, "error": "timeout"}


failures = 0

r = call("ping")
print("== ping ==")
print(json.dumps(r, ensure_ascii=False))
if not r.get("ok"):
    failures += 1

r = call("generate_fingerprint",
         {"os": ["windows"], "ff_version": 135, "locale": ["zh-CN"]})
print("\n== generate_fingerprint ==")
if r.get("ok"):
    s = r["result"]["summary"]
    print("  ok")
    print("  UA       :", s["user_agent"][:110])
    print("  platform :", s["platform"])
    print("  screen   :", s["screen"])
    print("  webgl    :", s["webgl_vendor"][:30], "/", s["webgl_renderer"][:40])
else:
    print("  FAIL:", json.dumps(r.get("error"), ensure_ascii=False)[:300])
    failures += 1

r = call("list_webgl", {"os": "windows"})
print("\n== list_webgl ==")
if r.get("ok"):
    cards = r["result"].get("cards", [])
    print(f"  ok, {len(cards)} cards")
    for c in cards[:3]:
        print("   ", c.get("vendor", "")[:30], "|", c.get("renderer", "")[:45], "|", c.get("weight"))
else:
    print("  FAIL:", json.dumps(r.get("error"), ensure_ascii=False)[:300])
    failures += 1

call("shutdown", rid="9", timeout=30)
try:
    proc.stdin.close()
    proc.wait(timeout=10)
except Exception:
    proc.terminate()

print("\nworker 退出码:", proc.returncode)
print("=" * 40)
print("结果:", "全部通过" if failures == 0 else f"{failures} 项失败")
sys.exit(1 if failures else 0)
