use std::process::Command;

#[cfg(windows)]
pub fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;

    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
pub fn hide_console_window(_command: &mut Command) {}
