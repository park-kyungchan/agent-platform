#![cfg(target_os = "linux")]
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let base = std::env::var_os("AP_S03_TEST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("pane-host-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn script(&self, source: &str) {
        let path = self.0.join("herdr");
        fs::write(&path, source).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ap"));
        cmd.arg("pane")
            .env_clear()
            .env("PATH", &self.0)
            .env("HERDR_ENV", "1");
        cmd
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn real_dispatch_and_fake_herdr_process_boundary() {
    let f = Scratch::new();
    let result = f
        .command()
        .arg("--help")
        .env_remove("HERDR_ENV")
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, ap::pane::HELP.as_bytes());
    assert!(result.stderr.is_empty());
    f.script("#!/bin/sh\nprintf '%s\\n' '{\"result\":{\"panes\":[{\"pane_id\":\"wA:p1\",\"focused\":true}]}}'\n");
    let result = f.command().arg("--focused").output().unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"wA:p1\n");
    assert!(result.stderr.is_empty());
    for (source, message) in [
        (
            "#!/bin/sh\nexit 1\n",
            "could not query the current Herdr session",
        ),
        (
            "#!/bin/sh\nprintf invalid\n",
            "could not parse the current Herdr session",
        ),
    ] {
        f.script(source);
        let result = f.command().arg("--focused").output().unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            format!("herdr_query_failed@pane:1: {message}\n")
        );
    }
}
