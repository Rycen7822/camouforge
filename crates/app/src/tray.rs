//! Windows 托盘图标 + 主窗口隐藏/恢复助手（仅 Windows 编译，main.rs 里 `#[cfg(windows)] mod tray`）。
//!
//! 实现要点：
//! - gpui 没有托盘 / hide / show API，托盘图标用 Win32 `Shell_NotifyIconW` 自建。
//! - 图标挂在一个隐藏窗口上；该窗口创建在主线程（gpui 的 GetMessageW 循环会把线程上
//!   所有窗口的消息派发到各自 WndProc），双击 / 右键菜单事件经异步通道回传 GPUI。
//! - 主窗口 HWND 在窗口创建时经 [`register_main_hwnd`] 记录；隐藏/恢复直接调
//!   `ShowWindow`（隐藏窗口不会触发 gpui 的「窗口关闭 → 退出」，进程保持常驻）。

use std::sync::{Arc, OnceLock};

use gpui::Global;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GetCursorPos, LoadImageW, PostMessageW, RegisterClassW, SetForegroundWindow, ShowWindow,
    TrackPopupMenu, HCURSOR, HICON, IMAGE_ICON, LR_SHARED, MF_STRING, SW_HIDE, SW_SHOW,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE, WM_LBUTTONDBLCLK, WM_NULL,
    WM_RBUTTONUP, WNDCLASSW, WNDCLASS_STYLES,
};

const TRAY_CLASS: &str = "CamouForge.TrayWnd";
const TRAY_ICON_ID: u32 = 1;
const TRAY_CALLBACK_MSG: u32 = 0x8001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    Show,
    Quit,
}

static TRAY_TX: OnceLock<async_channel::Sender<TrayEvent>> = OnceLock::new();

/// HWND 的整数形式：`*mut c_void` 不是 Send/Sync 不能进 static，isize 无损保存。
#[derive(Clone, Copy)]
struct RawHwnd(isize);

static MAIN_HWND: OnceLock<RawHwnd> = OnceLock::new();

pub struct TrayGlobal(pub Arc<Tray>);
impl Global for TrayGlobal {}

pub struct Tray {
    hwnd: HWND,
    events: async_channel::Receiver<TrayEvent>,
}

impl Tray {
    #[allow(clippy::arc_with_non_send_sync)] // HWND 仅作 Win32 句柄值使用，进程内单窗口
    pub fn init() -> Option<Arc<Self>> {
        static REGISTERED: OnceLock<bool> = OnceLock::new();
        REGISTERED.get_or_init(|| {
            let class_name = wide(TRAY_CLASS);
            let class = WNDCLASSW {
                style: WNDCLASS_STYLES(0),
                lpfnWndProc: Some(tray_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: module_instance(),
                hIcon: HICON::default(),
                hCursor: HCURSOR::default(),
                hbrBackground: HBRUSH::default(),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
            };
            unsafe { RegisterClassW(&class) != 0 }
        });

        let class_name = wide(TRAY_CLASS);
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class_name.as_ptr()),
                PCWSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                None,
                None,
                Some(module_instance()),
                None,
            )
        }
        .ok()?;

        // 托盘图标 16px，直接取 exe 资源 ID 1（与窗口图标同一枚）。
        // MAKEINTRESOURCE 语义：指针值即资源 ID，必须是 1（app.rc 只嵌了 ID 1）。
        #[allow(clippy::manual_dangling_ptr)] // 指针值 1 = 资源 ID，非真实悬垂解引用
        let hicon = unsafe {
            LoadImageW(
                Some(module_instance()),
                PCWSTR(1usize as *const u16),
                IMAGE_ICON,
                16,
                16,
                LR_SHARED,
            )
        }
        .ok()
        .map(|h| HICON(h.0))
        .unwrap_or_default();

        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_ICON_ID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = TRAY_CALLBACK_MSG;
        nid.hIcon = hicon;
        for (i, c) in wide("CamouForge")
            .iter()
            .take(nid.szTip.len().saturating_sub(1))
            .enumerate()
        {
            nid.szTip[i] = *c;
        }

        if !unsafe { Shell_NotifyIconW(NIM_ADD, &nid).as_bool() } {
            let _ = unsafe { DestroyWindow(hwnd) };
            return None;
        }

        let (tx, rx) = async_channel::unbounded();
        let _ = TRAY_TX.set(tx);
        Some(Arc::new(Tray { hwnd, events: rx }))
    }

    pub fn shutdown(&self) {
        self.events.close();
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.hwnd;
            nid.uID = TRAY_ICON_ID;
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            let _ = DestroyWindow(self.hwnd);
        }
    }

    pub fn events(&self) -> async_channel::Receiver<TrayEvent> {
        self.events.clone()
    }
}

pub fn register_main_hwnd(hwnd: HWND) {
    let _ = MAIN_HWND.set(RawHwnd(hwnd.0 as isize));
}

fn main_hwnd() -> Option<HWND> {
    MAIN_HWND.get().map(|h| HWND(h.0 as *mut core::ffi::c_void))
}

pub fn hide_main_window() {
    if let Some(hwnd) = main_hwnd() {
        let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

pub fn show_main_window() {
    if let Some(hwnd) = main_hwnd() {
        let _ = unsafe { ShowWindow(hwnd, SW_SHOW) };
        let _ = unsafe { SetForegroundWindow(hwnd) };
    }
}

const MENU_SHOW: usize = 1;
const MENU_QUIT: usize = 2;

/// 右键菜单：显示 / 退出。TPM_RETURNCMD 让 TrackPopupMenu 直接返回选中项，菜单在
/// 主线程模态弹出（内部派发线程消息，与原生应用行为一致）。
fn show_tray_menu(hwnd: HWND) {
    let Some(tx) = TRAY_TX.get() else {
        return;
    };
    unsafe {
        let Ok(hmenu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            MENU_SHOW,
            PCWSTR(wide("显示 CamouForge").as_ptr()),
        );
        let _ = AppendMenuW(hmenu, MF_STRING, MENU_QUIT, PCWSTR(wide("退出").as_ptr()));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            hmenu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(hmenu);
        // 菜单关闭后交还前台，避免残留激活状态。
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        match cmd.0 as usize {
            MENU_SHOW => {
                let _ = tx.try_send(TrayEvent::Show);
            }
            MENU_QUIT => {
                let _ = tx.try_send(TrayEvent::Quit);
            }
            _ => {}
        }
    }
}

/// 隐藏窗口的 WndProc：只处理托盘回调消息（lParam 携带实际鼠标消息），其余交默认。
unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == TRAY_CALLBACK_MSG {
        match lparam.0 as u32 {
            WM_LBUTTONDBLCLK => {
                if let Some(tx) = TRAY_TX.get() {
                    let _ = tx.try_send(TrayEvent::Show);
                }
            }
            WM_RBUTTONUP => show_tray_menu(hwnd),
            _ => {}
        }
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

fn module_instance() -> HINSTANCE {
    unsafe { GetModuleHandleW(PCWSTR::null()) }
        .unwrap_or_default()
        .into()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
