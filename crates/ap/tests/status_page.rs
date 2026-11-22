//! The systemd unit files and README section of the status page.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
#[test]
fn the_status_page_unit_files_and_readme_section() {
    let service = include_str!("../../../deploy/ap-status-page.service");
    let lines: Vec<&str> = service.lines().filter(|l| !l.starts_with('#')).collect();
    for want in [
        "Type=oneshot",
        "Nice=10",
        "Environment=HOME=/root",
        "TimeoutStartSec=120",
        "StateDirectory=agent-platform/status-page",
        "StateDirectoryMode=0755",
    ] {
        assert!(lines.contains(&want), "missing in service: {want}");
    }
    assert!(
        !service.contains("Restart="),
        "service must not set Restart="
    );

    let exec = lines
        .iter()
        .find_map(|l| l.strip_prefix("ExecStart="))
        .expect("missing ExecStart= in service");
    assert!(exec.starts_with("/bin/sh -c "), "{exec}");
    assert!(exec.contains("/usr/local/bin/ap status --text"), "{exec}");
    assert!(
        exec.contains("/var/lib/agent-platform/status-page/status.txt.new"),
        "{exec}"
    );
    assert!(
        exec.contains("/var/lib/agent-platform/status-page/status.txt"),
        "{exec}"
    );
    for forbidden in ["${", "$$", "%"] {
        assert!(
            !exec.contains(forbidden),
            "ExecStart contains {forbidden}: {exec}"
        );
    }

    let timer = include_str!("../../../deploy/ap-status-page.timer");
    let timer_lines: Vec<&str> = timer.lines().filter(|l| !l.starts_with('#')).collect();
    for want in [
        "[Timer]",
        "OnCalendar=*:0/5",
        "AccuracySec=10s",
        "Persistent=false",
        "WantedBy=timers.target",
    ] {
        assert!(timer_lines.contains(&want), "missing in timer: {want}");
    }

    let readme = include_str!("../../../deploy/README.md");
    let (_, section) = readme
        .split_once("## The status page")
        .expect("missing '## The status page' section in deploy/README.md");
    let section = section
        .split_once("\n## ")
        .map_or(section, |(head, _)| head);
    assert!(section.contains("ap-status-page.service"));
    assert!(section.contains("ap-status-page.timer"));
    assert!(section.contains("tailscale funnel"));
    let save_pos = section
        .find("serve status --json")
        .expect("missing 'serve status --json' in status page section");
    let bg_pos = section
        .find("--bg")
        .expect("missing '--bg' in status page section");
    assert!(
        save_pos < bg_pos,
        "'serve status --json' must appear before '--bg'"
    );
}
