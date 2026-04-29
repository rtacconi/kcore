//! Optional attach to a TTY path (e.g. `/dev/tty1`).

use std::fs::OpenOptions;

/// Duplicate an open TTY to stdin/stdout/stderr so crossterm and Ratatui use it.
pub fn attach_tty(tty: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let f = OpenOptions::new().read(true).write(true).open(tty)?;
        let fd = f.as_raw_fd();
        unsafe {
            for stream in [0, 1, 2] {
                if libc::dup2(fd, stream) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tty;
        return Err(std::io::Error::other(
            "--tty is only supported on Unix-like systems",
        ));
    }
    Ok(())
}
