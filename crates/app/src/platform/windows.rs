use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

pub fn assign_process_job() {
    let Ok(job) = win32job::Job::create() else {
        return;
    };
    use win32job::ExtendedLimitInfo;
    let mut info = ExtendedLimitInfo::default();
    info.limit_kill_on_job_close();
    if job.set_extended_limit_info(&info).is_ok() && job.assign_current_process().is_ok() {
        // Job 句柄必须活到进程结束，否则 KILL_ON_JOB_CLOSE 会立刻终止子进程树。
        std::mem::forget(job);
    }
}

pub fn main_hwnd(window: &gpui::Window) -> HWND {
    let null = HWND(std::ptr::null_mut());
    match HasWindowHandle::window_handle(window) {
        Ok(handle) => match handle.as_raw() {
            RawWindowHandle::Win32(handle) => HWND(handle.hwnd.get() as *mut core::ffi::c_void),
            _ => null,
        },
        Err(_) => null,
    }
}

pub fn fatal_message_box(title: &str, message: &str) {
    let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}
