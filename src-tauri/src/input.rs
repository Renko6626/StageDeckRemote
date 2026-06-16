//! 模拟鼠标 / 键盘输入（spec §11.8）。
//!
//! Windows 下用 Win32 `SendInput` 实现；其它平台返回 `Unsupported`，
//! 这样服务层可以在 Linux/macOS 上正常编译与联调（点击会被拒绝）。

#[derive(Debug)]
pub enum InputError {
    /// 非 Windows 平台拒绝模拟输入；在 Windows 构建下不会被构造。
    #[cfg_attr(windows, allow(dead_code))]
    Unsupported,
    /// Windows 下 SendInput 失败；在非 Windows 构建下不会被构造。
    #[cfg_attr(not(windows), allow(dead_code))]
    Failed(String),
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputError::Unsupported => write!(f, "当前平台不支持模拟输入（仅 Windows）"),
            InputError::Failed(e) => write!(f, "模拟输入失败: {e}"),
        }
    }
}

impl std::error::Error for InputError {}

/// 把物理屏幕坐标换算为 SendInput 绝对坐标（虚拟桌面 0..=65535）。
pub fn to_virtual_desktop_abs(
    x: i32,
    y: i32,
    vleft: i32,
    vtop: i32,
    vwidth: i32,
    vheight: i32,
) -> (i32, i32) {
    let w = (vwidth - 1).max(1) as i64;
    let h = (vheight - 1).max(1) as i64;
    let ax = ((x - vleft) as i64 * 65535 / w) as i32;
    let ay = ((y - vtop) as i64 * 65535 / h) as i32;
    (ax.clamp(0, 65535), ay.clamp(0, 65535))
}

#[cfg(windows)]
mod imp {
    use super::{to_virtual_desktop_abs, InputError};
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
        VK_LEFT, VK_RIGHT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    fn send(inputs: &[INPUT]) -> Result<(), InputError> {
        let sent = unsafe { SendInput(inputs, core::mem::size_of::<INPUT>() as i32) };
        if sent as usize == inputs.len() {
            Ok(())
        } else {
            Err(InputError::Failed("SendInput 注入数不足".into()))
        }
    }

    fn mouse_abs(ax: i32, ay: i32, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: ax,
                    dy: ay,
                    mouseData: 0,
                    dwFlags: MOUSE_EVENT_FLAGS(flags),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    pub fn click_screen(x: i32, y: i32) -> Result<(), InputError> {
        unsafe {
            // 记录当前鼠标位置以便还原。
            let mut prev = POINT::default();
            let _ = GetCursorPos(&mut prev);

            let vleft = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let vtop = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let vwidth = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let vheight = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            let (ax, ay) = to_virtual_desktop_abs(x, y, vleft, vtop, vwidth, vheight);

            let move_flags =
                MOUSEEVENTF_MOVE.0 | MOUSEEVENTF_ABSOLUTE.0 | MOUSEEVENTF_VIRTUALDESK.0;
            let inputs = [
                mouse_abs(ax, ay, move_flags),
                mouse_abs(ax, ay, MOUSEEVENTF_LEFTDOWN.0),
                mouse_abs(ax, ay, MOUSEEVENTF_LEFTUP.0),
            ];
            send(&inputs)?;

            // 还原鼠标到原位（减小光标出现在观众屏的概率）。
            let (rax, ray) = to_virtual_desktop_abs(prev.x, prev.y, vleft, vtop, vwidth, vheight);
            let _ = send(&[mouse_abs(rax, ray, move_flags)]);
        }
        Ok(())
    }

    fn key_press(vk: VIRTUAL_KEY) -> Result<(), InputError> {
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        send(&[down, up])
    }

    pub fn key_next() -> Result<(), InputError> {
        key_press(VK_RIGHT)
    }

    pub fn key_prev() -> Result<(), InputError> {
        key_press(VK_LEFT)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::InputError;

    pub fn click_screen(_x: i32, _y: i32) -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }

    pub fn key_next() -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }

    pub fn key_prev() -> Result<(), InputError> {
        Err(InputError::Unsupported)
    }
}

pub use imp::{click_screen, key_next, key_prev};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_monitor_corners() {
        // 虚拟桌面 1920x1080，原点 (0,0)
        assert_eq!(to_virtual_desktop_abs(0, 0, 0, 0, 1920, 1080), (0, 0));
        assert_eq!(to_virtual_desktop_abs(1919, 1079, 0, 0, 1920, 1080), (65535, 65535));
    }

    #[test]
    fn negative_origin_multimonitor() {
        // 左侧副屏使虚拟桌面原点为负
        assert_eq!(to_virtual_desktop_abs(-1920, 0, -1920, 0, 3840, 1080).0, 0);
    }

    #[test]
    fn clamps_outside() {
        let (ax, _) = to_virtual_desktop_abs(99999, 0, 0, 0, 1920, 1080);
        assert_eq!(ax, 65535);
    }
}
