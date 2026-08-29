from __future__ import annotations

import os
from typing import Dict, Optional

def _pid_alive(pid: int) -> bool:
    if os.name == "nt":
        import ctypes
        h = ctypes.windll.kernel32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        if not h:
            return False
        try:
            code = ctypes.c_ulong(0)
            if not ctypes.windll.kernel32.GetExitCodeProcess(h, ctypes.byref(code)):
                return False
            return code.value == 259  # STILL_ACTIVE
        finally:
            ctypes.windll.kernel32.CloseHandle(h)
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


# 用户关掉浏览器窗口后，camoufox 的进程树会继续无窗口存活（实例线程永远收不到
# disconnected，UI 永远显示运行中）。只能主动枚举：进程树还在但没有可见窗口
# = 用户已关窗。
if os.name == "nt":
    import ctypes
    from ctypes import wintypes

    class _PROCESSENTRY32W(ctypes.Structure):
        _fields_ = [
            ("dwSize", wintypes.DWORD),
            ("cntUsage", wintypes.DWORD),
            ("th32ProcessID", wintypes.DWORD),
            ("th32DefaultHeapID", ctypes.c_void_p),
            ("th32ModuleID", wintypes.DWORD),
            ("cntThreads", wintypes.DWORD),
            ("th32ParentProcessID", wintypes.DWORD),
            ("pcPriClassBase", ctypes.c_long),
            ("dwFlags", wintypes.DWORD),
            ("szExeFile", wintypes.WCHAR * 260),
        ]

    def _process_snapshot() -> Dict[int, tuple]:
        """pid → (ppid, exe 文件名小写)。"""
        k32 = ctypes.windll.kernel32
        k32.CreateToolhelp32Snapshot.restype = ctypes.c_void_p
        h = k32.CreateToolhelp32Snapshot(0x2, 0)  # TH32CS_SNAPPROCESS
        if not h or h == ctypes.c_void_p(-1).value:
            return {}
        out: Dict[int, tuple] = {}
        try:
            e = _PROCESSENTRY32W()
            e.dwSize = ctypes.sizeof(_PROCESSENTRY32W)
            ok = k32.Process32FirstW(ctypes.c_void_p(h), ctypes.byref(e))
            while ok:
                out[e.th32ProcessID] = (e.th32ParentProcessID, e.szExeFile.lower())
                ok = k32.Process32NextW(ctypes.c_void_p(h), ctypes.byref(e))
        finally:
            k32.CloseHandle(ctypes.c_void_p(h))
        return out

    def _find_browser_pid(driver_pid: int, exe_name: str) -> Optional[int]:
        """真实浏览器主进程 = 驱动进程（node）的同名直接子进程。"""
        exe_name = exe_name.lower()
        for pid, (ppid, name) in _process_snapshot().items():
            if ppid == driver_pid and name == exe_name:
                return pid
        return None

    def _tree_pids(root_pid: int, snap: Dict[int, tuple]) -> set:
        children: Dict[int, list] = {}
        for pid, (ppid, _) in snap.items():
            children.setdefault(ppid, []).append(pid)
        seen = {root_pid}
        stack = [root_pid]
        while stack:
            for c in children.get(stack.pop(), []):
                if c not in seen:
                    seen.add(c)
                    stack.append(c)
        return seen

    def _tree_has_visible_window(root_pid: int) -> bool:
        tree = _tree_pids(root_pid, _process_snapshot())
        u32 = ctypes.windll.user32
        found = []

        @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        def _cb(hwnd, _lp):
            if u32.IsWindowVisible(hwnd):
                pid = wintypes.DWORD(0)
                u32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                if pid.value in tree:
                    found.append(hwnd)
                    return False  # 一个可见窗口即够
            return True

        u32.EnumWindows(_cb, 0)
        return bool(found)
