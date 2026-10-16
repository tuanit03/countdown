//! Control FIFO: the GNOME hotkey writes `show` (or `hide`, `quit`) into it.

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Hide,
    Quit,
    /// `type 425`: enter digits as if typed (scripting/testing).
    Type(String),
    /// `enter`: same as pressing Enter.
    Enter,
}

pub struct Ctl {
    file: std::fs::File,
    pending: Vec<u8>,
}

pub fn default_path() -> PathBuf {
    if let Ok(p) = std::env::var("COUNTDOWN_CTL") {
        return p.into();
    }
    let rt = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(rt).join("countdown/ctl")
}

impl Ctl {
    pub fn open(path: &Path) -> io::Result<Ctl> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_fifo() => {}
            Ok(_) => {
                std::fs::remove_file(path)?;
                mkfifo(path)?;
            }
            Err(_) => mkfifo(path)?,
        }
        // Opening read+write keeps a writer alive, so the FIFO never reports EOF between hotkey presses.
        let file = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
        let fd = file.as_raw_fd();
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        Ok(Ctl { file, pending: Vec::new() })
    }

    pub fn fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }

    /// Drain whatever is readable and return complete commands.
    pub fn read(&mut self) -> Vec<Command> {
        use std::io::Read;
        let mut buf = [0u8; 256];
        loop {
            match (&self.file).read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.pending.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        let mut out = Vec::new();
        while let Some(i) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=i).collect();
            if let Some(c) = parse(&line) {
                out.push(c);
            }
        }
        if self.pending.len() > 1024 {
            self.pending.clear();
        }
        out
    }
}

fn mkfifo(path: &Path) -> io::Result<()> {
    let c = CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

pub fn parse(line: &[u8]) -> Option<Command> {
    let line = std::str::from_utf8(line).ok()?.trim();
    match line {
        "show" | "toggle" => Some(Command::Toggle),
        "hide" => Some(Command::Hide),
        "quit" => Some(Command::Quit),
        "enter" => Some(Command::Enter),
        _ => line.strip_prefix("type ").map(|d| Command::Type(d.trim().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn fifo_roundtrip_replaces_regular_file() {
        let dir = std::env::temp_dir().join(format!("countdown-ctl-{}", std::process::id()));
        let path = dir.join("ctl");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "stale").unwrap();
        let mut ctl = Ctl::open(&path).unwrap();
        assert!(ctl.read().is_empty());
        let mut w = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        w.write_all(b"show\nbogus\ntype 425\nqu").unwrap();
        assert_eq!(ctl.read(), vec![Command::Toggle, Command::Type("425".into())]);
        w.write_all(b"it\n").unwrap();
        assert_eq!(ctl.read(), vec![Command::Quit]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
