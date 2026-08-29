//! Windows 快捷方式创建与保存对话框（仅 Windows 模块）。
//!
//! - `pick_shortcut_path`：弹出系统「另存为」对话框，仅显示 `*.lnk`。
//! - `create_shortcut`：用 COM `IShellLinkW` + `IPersistFile` 生成 `.lnk`。

use std::path::{Path, PathBuf};

use windows::core::{w, Interface, HSTRING};
use windows::Win32::Foundation::{HWND, S_OK};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileSaveDialog, IFileSaveDialog, IShellLinkW, ShellLink, SIGDN_FILESYSPATH,
};

const RPC_E_CHANGED_MODE: i32 = -2147417850; // 0x80010106

pub fn pick_shortcut_path(default_name: &str) -> Option<PathBuf> {
    let dialog: IFileSaveDialog = unsafe {
        // 初始化 COM；若已被其他线程以不同模式初始化则忽略错误。
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL).ok()?
    };

    let filter_name = w!("快捷方式 (*.lnk)");
    let filter_spec = w!("*.lnk");
    let filters = [COMDLG_FILTERSPEC {
        pszName: windows::core::PCWSTR(filter_name.as_ptr()),
        pszSpec: windows::core::PCWSTR(filter_spec.as_ptr()),
    }];

    unsafe {
        dialog.SetFileTypes(&filters).ok()?;
        dialog.SetDefaultExtension(w!("lnk")).ok()?;
        dialog.SetFileName(&HSTRING::from(default_name)).ok()?;
    }

    unsafe {
        match dialog.Show(Some(HWND(std::ptr::null_mut()))) {
            Ok(_) => {}
            Err(e) => {
                // ERROR_CANCELLED = 0x800704C7，用户点取消或关闭对话框。
                if e.code().0 == 0x800704C7u32 as i32 {
                    return None;
                }
                return None;
            }
        }
    }

    let item = unsafe { dialog.GetResult() }.ok()?;
    let path_pwstr = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;

    let path = unsafe {
        let mut len = 0usize;
        while *path_pwstr.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(path_pwstr.0, len);
        PathBuf::from(String::from_utf16_lossy(slice))
    };

    unsafe {
        CoTaskMemFree(Some(path_pwstr.0 as *const _));
    }

    Some(path)
}

pub fn create_shortcut(
    exe: &Path,
    args: &str,
    lnk_path: &Path,
    description: &str,
) -> Result<(), String> {
    let exe_h = HSTRING::from(exe.to_string_lossy().as_ref());
    let args_h = HSTRING::from(args);
    let desc_h = HSTRING::from(description);
    let lnk_h = HSTRING::from(lnk_path.to_string_lossy().as_ref());

    unsafe {
        let init_hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        if init_hr.is_err() && init_hr.0 != S_OK.0 && init_hr.0 != RPC_E_CHANGED_MODE {
            return Err(format!("COM 初始化失败: {init_hr:?}"));
        }

        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_ALL)
            .map_err(|e| format!("创建 ShellLink 失败: {e}"))?;

        shell_link
            .SetPath(windows::core::PCWSTR(exe_h.as_ptr()))
            .map_err(|e| format!("SetPath 失败: {e}"))?;
        shell_link
            .SetArguments(windows::core::PCWSTR(args_h.as_ptr()))
            .map_err(|e| format!("SetArguments 失败: {e}"))?;
        shell_link
            .SetDescription(windows::core::PCWSTR(desc_h.as_ptr()))
            .map_err(|e| format!("SetDescription 失败: {e}"))?;
        shell_link
            .SetIconLocation(windows::core::PCWSTR(exe_h.as_ptr()), 0)
            .map_err(|e| format!("SetIconLocation 失败: {e}"))?;

        let persist: IPersistFile = shell_link
            .cast()
            .map_err(|e| format!("获取 IPersistFile 失败: {e}"))?;
        persist
            .Save(windows::core::PCWSTR(lnk_h.as_ptr()), true)
            .map_err(|e| format!("保存快捷方式失败: {e}"))?;
    }

    Ok(())
}
