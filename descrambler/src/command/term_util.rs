/// Method to clear CTRL+C printing interrupting progress bars
pub fn disable_echo_ctl() {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        use libc::{tcgetattr, tcsetattr, termios, ECHOCTL, TCSANOW};
        let fd = std::io::stdin().as_raw_fd();

        unsafe {
            let mut term: termios = std::mem::zeroed();
            if tcgetattr(fd, &mut term) == 0 {
                term.c_lflag &= !ECHOCTL;
                tcsetattr(fd, TCSANOW, &term);
            }
        }
    }
    #[cfg(windows)]
    {
        use windows::{
            Win32::System::Console::GetStdHandle,
            Win32::System::Console::STD_INPUT_HANDLE,
            Win32::System::Console::{
                GetConsoleMode, SetConsoleMode, ENABLE_PROCESSED_INPUT,
            },
        };

        unsafe {
            if let Ok(h) = GetStdHandle(STD_INPUT_HANDLE) {
                let mut mode = 0;
                if GetConsoleMode(h, &mut mode).is_ok() {
                    let new_mode = mode & !ENABLE_PROCESSED_INPUT;
                    let _ = SetConsoleMode(h, new_mode);
                }
            }
        }
    }
}