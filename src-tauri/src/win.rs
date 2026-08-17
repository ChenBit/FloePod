//! Win32 辅助：不抢焦点显示窗口、修饰键状态。

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SW_SHOWNOACTIVATE,
};

const PRESSED: i16 = -32768; // GetAsyncKeyState 高位为 1 时的返回值（i16 下溢表示）

fn key_down(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) == PRESSED }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifierState {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

pub fn modifier_state() -> ModifierState {
    ModifierState {
        ctrl: key_down(VK_CONTROL),
        shift: key_down(VK_SHIFT),
        alt: key_down(VK_MENU),
    }
}

/// SW_SHOWNOACTIVATE 显示 + 无激活置顶：
/// 面板出现时不从用户当前应用抢走键盘焦点。
pub fn show_no_activate(hwnd: isize) {
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        );
    }
}
