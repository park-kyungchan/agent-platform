//! The forks of a test binary and the advisory locks of its tests (`tests/work.rs`, `tests/unit_run.rs`).
//!
//! A child forked by one thread holds a copy of every descriptor of the process from its fork until its
//! `exec` (each is close-on-exec), and with that copy any `flock` held through one: a lock a test closed
//! in that window stays held, and the test's next attempt on it answers as if a run held it. So every
//! fork of a test binary is made inside `forking` (std's `spawn`, `output` and `status` return only after
//! the child's `exec`), and a test that takes a lock again after a close first calls `settled`, which
//! returns once no fork of the binary is between its fork and its `exec`.
#![allow(
    dead_code,
    reason = "each test binary that includes this module uses only part of it"
)]
use std::{
    process::{Command, Stdio},
    sync::{OnceLock, RwLock},
    time::Duration,
};

static GATE: RwLock<()> = RwLock::new(());

/// Runs `f`, which forks, with the gate held shared: the gate is let go only after the child's `exec`.
pub fn forking<T>(f: impl FnOnce() -> T) -> T {
    let _shared = GATE.read().unwrap_or_else(|e| e.into_inner());
    f()
}
/// Returns once no child forked inside `forking` is between its fork and its `exec`.
pub fn settled() {
    drop(GATE.write().unwrap_or_else(|e| e.into_inner()));
}
/// With `AP_TEST_FORK_GAP` set, one thread of the binary forks without end: each child walks a `PATH`
/// of directories that do not exist before it finds `true`, which holds the window between its fork
/// and its `exec` open for milliseconds (the gap that shows the race; the workspace has no `unsafe`).
pub fn gap() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if std::env::var_os("AP_TEST_FORK_GAP").is_none() {
        return;
    }
    STARTED.get_or_init(|| {
        let path: Vec<String> = (0..4000)
            .map(|n| format!("/nonexistent-ap-test-gap/{n}"))
            .chain(["/usr/bin".into(), "/bin".into()])
            .collect();
        let path = path.join(":");
        std::thread::spawn(move || {
            loop {
                let _ = forking(|| {
                    Command::new("true")
                        .env_clear()
                        .env("PATH", &path)
                        .stdin(Stdio::null())
                        .status()
                });
                std::thread::sleep(Duration::from_millis(2));
            }
        });
    });
}
