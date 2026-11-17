// This crate is the sanctioned process spawner; wrapping tokio's Command here is the point.
#![allow(clippy::disallowed_types)]

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::Stdio;

/// A process builder. Kills the child on drop and never shows a console window
/// on Windows. Actual OS sandboxing lands in M3; this is the single spawner.
pub struct Command {
    inner: tokio::process::Command,
}

// CREATE_NO_WINDOW
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

impl Command {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        let mut inner = tokio::process::Command::new(program);
        inner.kill_on_drop(true);
        #[cfg(windows)]
        inner.creation_flags(NO_WINDOW);
        Self { inner }
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.inner.arg(arg);
        self
    }

    pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        self.inner.args(args);
        self
    }

    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.inner.current_dir(dir);
        self
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.inner.env(key, value);
        self
    }

    pub fn stdin(&mut self, cfg: Stdio) -> &mut Self {
        self.inner.stdin(cfg);
        self
    }

    pub fn stdout(&mut self, cfg: Stdio) -> &mut Self {
        self.inner.stdout(cfg);
        self
    }

    pub fn stderr(&mut self, cfg: Stdio) -> &mut Self {
        self.inner.stderr(cfg);
        self
    }

    pub fn spawn(&mut self) -> io::Result<tokio::process::Child> {
        self.inner.spawn()
    }
}
