"""CamouForge worker launch 手动验证：启动一个 headless 实例 → list → stop → shutdown。

必须用项目 venv 的 Python 运行（worker 依赖 camoufox/browserforge）：
    .venv/Scripts/python.exe launch_test.py
executable_path 不写死：由 worker 自动探测（CAMOUFOX_EXECUTABLE_PATH →
仓库兄弟目录 camoufox* → pip 缓存）。
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
    [str(PYTHON), "-u", str(WORKER)],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    text=True, bufsize=1, encoding="utf-8",
)

def call(method, params=None, rid="1", timeout=240):
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
            continue
        if msg.get("id") == rid:
            return msg
        if msg.get("event"):
            print("  [事件]", msg["event"], json.dumps(msg.get("data", {}), ensure_ascii=False)[:150])
    return {"id": rid, "ok": False, "error": "timeout"}

profile = {
    "id": "launch-test-1",
    "launch": {
        "headless": "headless",
    },
}

print("== launch（executable_path 由 worker 自动探测）==")
r = call("launch", {"profile": profile}, rid="2", timeout=240)
print(json.dumps(r, ensure_ascii=False, indent=2))

time.sleep(3)

print("== list ==")
r2 = call("list", rid="3", timeout=30)
print(json.dumps(r2, ensure_ascii=False, indent=2))

print("== stop ==")
r3 = call("stop", {"profile_id": "launch-test-1"}, rid="4", timeout=60)
print(json.dumps(r3, ensure_ascii=False, indent=2))

call("shutdown", rid="9", timeout=30)
try:
    proc.wait(timeout=15)
except Exception:
    proc.terminate()
print("worker exit:", proc.returncode)
