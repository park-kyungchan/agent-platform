//! `ap quota` against a fake host (`specs/ap/quota.md`): the fake answers `systemctl`, `/proc/stat` and
//! the control group's file itself, and nothing here starts a unit, sets a property or reads the
//! host's own load.
use ap::host::{Answer, Failure, Reply, System};
use ap::quota::{execute, exit_status, load};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    io::ErrorKind,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const SLICE: &str = "aptest.slice";
const CPU_STAT: &str = "/sys/fs/cgroup/aptest.slice/cpu.stat";

struct Fake {
    dir: PathBuf,
    policy: Option<PathBuf>,
    units: bool,
    /// The successive readings of `/proc/stat`; None is a failed read.
    proc: RefCell<VecDeque<Option<String>>>,
    /// The successive files of each path under `/sys/fs/cgroup`; no queue is an absent file.
    files: RefCell<BTreeMap<String, VecDeque<Result<String, ErrorKind>>>>,
    /// The successive answers of `ControlGroup`; None is a failed call.
    groups: VecDeque<Option<String>>,
    /// The text `CPUQuotaPerSecUSec` prints; None is a failed call.
    quota: Option<String>,
    /// `set-property` changes `quota` (else it changes nothing) and succeeds (else the call fails).
    set_works: bool,
    set_call_ok: bool,
    /// How long one `pause` lasts on the clock.
    gap: Duration,
    calls: Vec<Vec<String>>,
    /// The bound in milliseconds of each call, and the time waited by `pause`.
    bounds: Vec<u64>,
    waited: Duration,
    reads: RefCell<Vec<String>>,
    base: Instant,
    clock: Cell<Duration>,
}
impl Fake {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ap-quota-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = Fake {
            dir,
            policy: None,
            units: true,
            proc: RefCell::default(),
            files: RefCell::default(),
            groups: VecDeque::new(),
            quota: Some("infinity".into()),
            set_works: true,
            set_call_ok: true,
            gap: Duration::from_secs(10),
            calls: vec![],
            bounds: vec![],
            waited: Duration::ZERO,
            reads: RefCell::default(),
            base: Instant::now(),
            clock: Cell::default(),
        };
        f.policy_is(&json!({"quota": {"slice": SLICE}}));
        f
    }
    fn policy_is(&mut self, v: &Value) {
        let file = self.dir.join("policy.json");
        std::fs::write(&file, v.to_string()).unwrap();
        self.policy = Some(file);
    }
    /// Two readings: `cpus` CPUs at `busy` per cent of all CPUs, the slice at `slice_use` per cent of one
    /// CPU, over the ten seconds between them.
    fn scenario(&mut self, cpus: u64, busy: u64, slice_use: u64) {
        let (user, idle) = (busy * 10 * cpus, (100 - busy) * 10 * cpus);
        self.proc = RefCell::new(
            [(1_000, 9_000), (1_000 + user, 9_000 + idle)]
                .map(|(u, i)| Some(stat(cpus, [u, 0, 0, i, 0, 0, 0, 0, 0, 0])))
                .into(),
        );
        self.usage(&[5_000_000, 5_000_000 + slice_use * 100_000]);
        self.groups = VecDeque::from([Some(format!("/{SLICE}\n")), Some(format!("/{SLICE}\n"))]);
    }
    fn usage(&mut self, counts: &[u64]) {
        let queue = counts
            .iter()
            .map(|n| Ok(format!("usage_usec {n}\nuser_usec 1\n")));
        self.files
            .borrow_mut()
            .insert(CPU_STAT.into(), queue.collect());
    }
    fn group(&mut self, answer: &str) {
        self.groups = VecDeque::from([Some(answer.to_owned()), Some(answer.to_owned())]);
    }
    /// The answer of one turn.
    fn turn(&mut self) -> Result<Value, Failure> {
        execute(self)
    }
    fn ok(&mut self) -> Value {
        self.turn()
            .unwrap_or_else(|e| panic!("{} {} {}", e.code, e.field, e.detail))
    }
    fn refused(&mut self) -> String {
        match self.turn() {
            Err(f) => format!("{}@{}", f.code, f.field),
            Ok(v) => panic!("expected a failure, got {v}"),
        }
    }
    fn sets(&self) -> Vec<Vec<String>> {
        self.calls
            .iter()
            .filter(|c| c[0] == "set-property")
            .cloned()
            .collect()
    }
    /// What a turn read besides the policy file.
    fn host_reads(&self) -> Vec<String> {
        let policy = self.policy.as_ref().map(|p| p.to_str().unwrap().to_owned());
        self.reads
            .borrow()
            .iter()
            .filter(|r| Some(*r) != policy.as_ref())
            .cloned()
            .collect()
    }
    /// Nothing was called and nothing but the policy was read since the last check.
    fn nothing_else(&mut self) {
        assert!(self.calls.is_empty() && self.host_reads().is_empty());
        self.reads.borrow_mut().clear();
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn stat(cpus: u64, t: [u64; 10]) -> String {
    let t = t.map(|n| n.to_string()).join(" ");
    let mut text = format!("cpu  {t}\n");
    for n in 0..cpus {
        text += &format!("cpu{n} 1 2 3 4 5 6 7 8 9 10\n");
    }
    text + "intr 1 2 3\nctxt 4\nbtime 5\n"
}
fn broken(program: &str) -> Failure {
    Failure {
        code: "process_failed",
        field: program.into(),
        detail: Value::Null,
    }
}
fn reply(text: &str) -> Answer {
    Ok(Reply {
        success: true,
        bytes: text.as_bytes().to_vec(),
        stdout: text.as_bytes().to_vec(),
    })
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], _: &Value, ms: u64) -> Answer {
        assert_eq!(program, "systemctl", "a call to {program}");
        self.calls.push(args.to_vec());
        self.bounds.push(ms);
        let words: Vec<&str> = args.iter().map(String::as_str).collect();
        match words[..] {
            ["show", "-p", "ControlGroup", "--value", SLICE] => {
                match self.groups.pop_front().expect("one answer too many") {
                    Some(answer) => reply(&answer),
                    None => Err(broken(program)),
                }
            }
            ["show", "-p", "CPUQuotaPerSecUSec", "--value", SLICE] => match &self.quota {
                Some(text) => reply(&format!("{text}\n")),
                None => Err(broken(program)),
            },
            ["set-property", "--runtime", SLICE, property] => {
                if self.set_works {
                    let n: u64 = property
                        .strip_prefix("CPUQuota=")
                        .unwrap()
                        .strip_suffix('%')
                        .unwrap()
                        .parse()
                        .unwrap();
                    self.quota = Some(format!("{}.{:06}s", n / 100, n % 100 * 10_000));
                }
                if self.set_call_ok {
                    reply("")
                } else {
                    Err(broken(program))
                }
            }
            _ => panic!("a call that is none of the three forms: {words:?}"),
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.reads.borrow_mut().push(path.to_owned());
        if path == "/proc/stat" {
            let next = self
                .proc
                .borrow_mut()
                .pop_front()
                .expect("one reading too many");
            return next
                .map(String::into_bytes)
                .ok_or_else(|| ErrorKind::PermissionDenied.into());
        }
        if path.starts_with("/sys/fs/cgroup") {
            let next = self
                .files
                .borrow_mut()
                .get_mut(path)
                .and_then(VecDeque::pop_front);
            return match next {
                Some(Ok(text)) => Ok(text.into_bytes()),
                Some(Err(kind)) => Err(kind.into()),
                None => Err(ErrorKind::NotFound.into()),
            };
        }
        assert_eq!(
            Some(path),
            self.policy.as_ref().and_then(|p| p.to_str()),
            "a read of {path}"
        );
        std::fs::read(path)
    }
    fn units(&self) -> bool {
        self.units
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.policy.clone()
    }
    fn now(&self) -> Instant {
        self.base + self.clock.get()
    }
    fn pause(&mut self, duration: Duration) {
        assert_eq!(duration, Duration::from_secs(10));
        self.waited += duration;
        self.clock.set(self.clock.get() + self.gap);
    }
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_794_648_600)
    }
}
fn ap(args: &[&str], policy: Option<&PathBuf>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ap"));
    command.args(args).env_clear().stdin(Stdio::null());
    command.env(
        "AP_POLICY_FILE",
        policy
            .cloned()
            .unwrap_or_else(|| "/nonexistent/none.json".into()),
    );
    command.output().unwrap()
}
fn error_of(out: &std::process::Output) -> Value {
    assert!(out.stdout.is_empty());
    serde_json::from_slice(&out.stderr).unwrap()
}

// spec: specs/ap/quota.md#R1
#[test]
fn r1_an_argument_is_usage_before_any_reading_and_a_turn_writes_and_locks_nothing() {
    let out = ap(&["quota", "now"], None);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["error"]["code"], "usage");
    assert_eq!(error_of(&out)["error"]["field"], "args");
    let help = String::from_utf8(ap(&["--help"], None).stdout).unwrap();
    assert!(
        help.contains("  quota: ap quota (README \"The CPU quota\")\n"),
        "{help}"
    );
    let mut f = Fake::new();
    f.scenario(2, 20, 30);
    let before: Vec<_> = std::fs::read_dir(&f.dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    f.ok();
    let after: Vec<_> = std::fs::read_dir(&f.dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(before, after, "a turn wrote a file");
}

// spec: specs/ap/quota.md#R2
#[test]
fn r2_the_members_have_their_defaults_and_ranges() {
    let mut f = Fake::new();
    // The defaults: busy limit 85, margin 5, ceiling 170, floor 60.
    f.scenario(4, 0, 0);
    assert_eq!(f.ok()["quota"], 170);
    f.scenario(2, 100, 0);
    assert_eq!(f.ok()["quota"], 60);
    // Every member given: (50 - 10) x 2 CPUs - 0 outside = 80, between 20 and 100.
    f.policy_is(&json!({"quota": {"slice": SLICE, "busy_limit": 50, "margin": 10, "ceiling": 100, "floor": 20}}));
    f.scenario(2, 0, 0);
    assert_eq!(f.ok()["quota"], 80);
    // A 32-character name and the bounds of every range are valid.
    let long = format!("{}.slice", "a".repeat(32));
    for q in [
        json!({"slice": long}),
        json!({"slice": "a1.slice", "busy_limit": 10, "margin": 0, "ceiling": 10, "floor": 10}),
        json!({"slice": "a1.slice", "busy_limit": 100, "margin": 50, "ceiling": 6400, "floor": 6400}),
    ] {
        f.policy_is(&json!({"quota": q.clone()}));
        f.units = false;
        assert_eq!(f.ok(), json!({"skipped": "no service manager"}), "{q}");
    }
}
// spec: specs/ap/quota.md#R2
#[test]
fn r2_a_member_out_of_its_form_is_policy_invalid_and_nothing_is_read() {
    let long = format!("{}.slice", "a".repeat(33));
    let cases = [
        json!({}),
        json!({"slice": 1}),
        json!({"slice": ""}),
        json!({"slice": ".slice"}),
        json!({"slice": "agents-x.slice"}),
        json!({"slice": "Agents.slice"}),
        json!({"slice": "a1.service"}),
        json!({"slice": "a1"}),
        json!({"slice": "-a.slice"}),
        json!({"slice": "a b.slice"}),
        json!({"slice": "../a.slice"}),
        json!({"slice": long}),
        json!({"slice": "a1.slice", "limit": 5}),
        json!({"slice": "a1.slice", "busy_limit": 9}),
        json!({"slice": "a1.slice", "busy_limit": 101}),
        json!({"slice": "a1.slice", "busy_limit": "85"}),
        json!({"slice": "a1.slice", "busy_limit": 85.5}),
        json!({"slice": "a1.slice", "busy_limit": null}),
        json!({"slice": "a1.slice", "margin": -1}),
        json!({"slice": "a1.slice", "margin": 51}),
        json!({"slice": "a1.slice", "ceiling": 9}),
        json!({"slice": "a1.slice", "ceiling": 6401}),
        json!({"slice": "a1.slice", "floor": 9}),
        json!({"slice": "a1.slice", "ceiling": 100, "floor": 101}),
        json!({"slice": "a1.slice", "floor": 171}),
        json!([]),
        json!("a1.slice"),
    ];
    for q in cases {
        let mut f = Fake::new();
        f.policy_is(&json!({"quota": q.clone()}));
        let answer = f.turn().err().unwrap_or_else(|| panic!("{q} was taken"));
        assert_eq!(
            (answer.code, answer.field.as_str()),
            ("policy_invalid", "policy"),
            "{q}"
        );
        assert!(
            answer.detail.as_str().unwrap().starts_with("quota"),
            "{q}: {}",
            answer.detail
        );
        f.nothing_else();
    }
    for text in ["", "not json", "[]", "3"] {
        let mut f = Fake::new();
        std::fs::write(f.policy.as_ref().unwrap(), text).unwrap();
        let answer = f.turn().err().unwrap();
        assert_eq!(
            (
                answer.code,
                answer.detail.as_str().unwrap().split(' ').next().unwrap()
            ),
            ("policy_invalid", "file")
        );
        f.nothing_else();
    }
    // A file that cannot be read is invalid, a missing one is no policy (R3).
    let mut f = Fake::new();
    f.policy = Some(f.dir.clone());
    assert!(f.refused().starts_with("policy_invalid"));
    // No unit of the service manager is asked for before the refusal even with no service manager.
    let mut f = Fake::new();
    f.units = false;
    f.policy_is(&json!({"quota": {"slice": "a1.slice", "margin": 99}}));
    assert_eq!(f.refused(), "policy_invalid@policy");
}

// spec: specs/ap/quota.md#R3
#[test]
fn r3_no_policy_no_member_or_no_service_manager_is_skipped_with_no_other_call() {
    let mut f = Fake::new();
    f.policy = None;
    assert_eq!(f.ok(), json!({"skipped": "no quota"}));
    f.nothing_else();
    f.policy = Some(f.dir.join("absent.json"));
    assert_eq!(f.ok(), json!({"skipped": "no quota"}));
    f.nothing_else();
    f.policy_is(&json!({"models": {"allowed": ["opus"], "never": []}, "accounts": {}}));
    assert_eq!(f.ok(), json!({"skipped": "no quota"}));
    f.nothing_else();
    f.policy_is(&json!({"quota": {"slice": SLICE}}));
    f.units = false;
    assert_eq!(f.ok(), json!({"skipped": "no service manager"}));
    f.nothing_else();
    // The built binary: a file that is absent, and one without the member, with no service manager involved.
    let absent = ap(&["quota"], None);
    assert_eq!(
        (absent.status.code(), absent.stdout),
        (Some(0), b"{\"skipped\":\"no quota\"}\n".to_vec())
    );
    let without = f.dir.join("without.json");
    std::fs::write(&without, "{}").unwrap();
    assert_eq!(
        ap(&["quota"], Some(&without)).stdout,
        b"{\"skipped\":\"no quota\"}\n"
    );
    let invalid = f.dir.join("invalid.json");
    std::fs::write(&invalid, "{\"quota\":{\"slice\":\"a-b.slice\"}}").unwrap();
    let out = ap(&["quota"], Some(&invalid));
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["error"]["code"], "policy_invalid");
}

// spec: specs/ap/quota.md#R4
#[test]
fn r4_idle_is_the_fourth_number_alone_and_the_cpus_are_the_cpu_lines() {
    let mut f = Fake::new();
    f.group("");
    // user nice system idle iowait irq softirq steal guest guest_nice: over the interval idle +500,
    // iowait +500, steal +100, guest +9999 (counted in user already).
    f.proc = RefCell::new(VecDeque::from([
        Some(stat(3, [100, 10, 20, 500, 40, 5, 6, 7, 8, 9])),
        Some(stat(3, [100, 10, 20, 1_000, 540, 5, 6, 107, 10_007, 9])),
    ]));
    let answer = f.ok();
    // total +1100 of which idle +500: busy 55 per cent of the 3 CPUs.
    assert_eq!(
        (answer["cpus"].clone(), answer["busy"].clone()),
        (json!(3), json!(55))
    );
    // The result is one function: `load` alone says the same.
    let mut g = Fake::new();
    g.group("");
    g.proc = RefCell::new(VecDeque::from([
        Some(stat(3, [100, 10, 20, 500, 40, 5, 6, 7, 8, 9])),
        Some(stat(3, [100, 10, 20, 1_000, 540, 5, 6, 107, 10_007, 9])),
    ]));
    let l = load(&mut g, SLICE).unwrap();
    assert_eq!((l.cpus, l.busy, l.slice_use, l.outside), (3, 55, 0, 165));
    // 2 CPUs at 75 per cent.
    f.scenario(2, 75, 0);
    assert_eq!(f.ok()["busy"], 75);
}

// spec: specs/ap/quota.md#R5
#[test]
fn r5_the_slice_use_is_its_usage_over_the_time_between_the_readings() {
    let mut f = Fake::new();
    f.scenario(2, 50, 40);
    let answer = f.ok();
    assert_eq!(answer["slice_use"], 40);
    assert_eq!(f.host_reads().iter().filter(|r| *r == CPU_STAT).count(), 2);
    // The time comes from the clock of the host: twenty seconds between the readings, half the use.
    let mut f = Fake::new();
    f.scenario(2, 50, 40);
    f.gap = Duration::from_secs(20);
    assert_eq!(f.ok()["slice_use"], 20);
    // A count that is lower the second time, an absent file and one answer that is not the slice's are no use.
    for counts in [&[9_000_000u64, 5_000_000][..], &[5_000_000]] {
        let mut f = Fake::new();
        f.scenario(2, 50, 40);
        f.usage(counts);
        assert_eq!(f.ok()["slice_use"], 0);
    }
    let mut f = Fake::new();
    f.scenario(2, 50, 40);
    f.files.borrow_mut().clear();
    assert_eq!(f.ok()["slice_use"], 0);
    let mut f = Fake::new();
    f.scenario(2, 50, 40);
    f.groups = VecDeque::from([Some(format!("/{SLICE}\n")), Some(String::new())]);
    assert_eq!(f.ok()["slice_use"], 0);
    // The control group of a slice that has no unit, or any form that is not `/<slice>`, is never a path.
    for answer in [
        "",
        "\n",
        "/",
        "/\n",
        "/other.slice\n",
        "/aptest.slice/sub\n",
        "/system.slice\n",
        "/aptest.slice",
        "//aptest.slice\n",
        " /aptest.slice\n",
        "/aptest.slice \n",
        "../aptest.slice\n",
    ] {
        let mut f = Fake::new();
        f.scenario(2, 50, 40);
        f.group(answer);
        let got = f.ok();
        assert_eq!(got["slice_use"], 0, "{answer:?}");
        assert_eq!(got["outside"], 100, "{answer:?}");
        assert_eq!(f.host_reads(), ["/proc/stat", "/proc/stat"], "{answer:?}");
    }
}

// spec: specs/ap/quota.md#R6
#[test]
fn r6_outside_is_the_busy_cpus_less_the_slice_never_below_zero() {
    for (cpus, busy, slice, outside) in [
        (2, 75, 40, 110),
        (2, 20, 30, 10),
        (2, 20, 20, 20),
        (2, 20, 40, 0),
        (4, 50, 0, 200),
        (2, 10, 100, 0),
    ] {
        let mut f = Fake::new();
        f.scenario(cpus, busy, slice);
        let answer = f.ok();
        assert_eq!(answer["outside"], outside, "{cpus} {busy} {slice}");
    }
    // The use that cannot be read makes outside larger, never smaller.
    let mut f = Fake::new();
    f.scenario(2, 75, 40);
    f.group("");
    assert_eq!(f.ok()["outside"], 150);
}

// spec: specs/ap/quota.md#R7
#[test]
fn r7_the_value_now_is_read_in_the_forms_the_service_manager_prints() {
    for (text, percent) in [
        ("infinity", Value::Null),
        ("100ms", json!(10)),
        ("600ms", json!(60)),
        ("1s", json!(100)),
        ("1.050000s", json!(105)),
        ("1.700000s", json!(170)),
        ("1.7s", json!(170)),
        ("1min", json!(6_000)),
        ("1min 4s", json!(6_400)),
        ("1s 500ms", json!(150)),
    ] {
        let mut f = Fake::new();
        f.scenario(2, 20, 30);
        f.quota = Some(text.into());
        assert_eq!(f.ok()["before"], percent, "{text}");
    }
    // Not a whole number of per cent: read as it is and never equal to the value computed.
    let mut f = Fake::new();
    f.scenario(2, 50, 0);
    f.group("");
    f.quota = Some("600ms".into());
    let answer = f.ok();
    assert_eq!(
        (answer["quota"].clone(), answer["set"].clone()),
        (json!(60), json!(false))
    );
    let mut f = Fake::new();
    f.scenario(2, 50, 0);
    f.group("");
    f.quota = Some("0.605000s".into());
    assert_eq!(f.ok()["set"], true);
    for text in [
        "",
        "1h",
        "abc",
        "1.5min",
        "5us",
        "-1s",
        "+1s",
        "1.2345678s",
        "1 s",
        "1..5s",
        ".5s",
        "1.s",
        "1.5ms",
        "10ms 500us",
        "s",
        "99999999999999999999s",
        "infinity 1s",
    ] {
        let mut f = Fake::new();
        f.scenario(2, 20, 30);
        f.quota = Some(text.into());
        assert_eq!(f.refused(), "reading_failed@quota_now", "{text:?}");
        assert!(f.sets().is_empty());
    }
}

// spec: specs/ap/quota.md#R8
#[test]
fn r8_a_reading_that_fails_is_reading_failed_and_nothing_is_set() {
    type Break = fn(&mut Fake);
    let cases: [(&str, Break); 12] = [
        ("proc_stat", |f| f.proc.borrow_mut()[0] = None),
        ("proc_stat", |f| f.proc.borrow_mut()[1] = None),
        ("proc_stat", |f| {
            f.proc.borrow_mut()[0] = Some("intr 1\n".into())
        }),
        ("proc_stat", |f| {
            f.proc.borrow_mut()[1] = Some("cpu  1 2 3 4 5 6 7\ncpu0 1 2\n".into())
        }),
        ("proc_stat", |f| {
            f.proc.borrow_mut()[0] = Some("cpu  1 2 3 4 5 6 7 8\n".into())
        }),
        ("proc_stat", |f| {
            let same = f.proc.borrow()[0].clone();
            f.proc.borrow_mut()[1] = same;
        }),
        ("control_group", |f| f.groups[0] = None),
        ("control_group", |f| f.groups[1] = None),
        ("quota_now", |f| f.quota = None),
        ("cpu_stat", |f| {
            f.files.borrow_mut().insert(
                CPU_STAT.into(),
                VecDeque::from([Err(ErrorKind::PermissionDenied)]),
            );
        }),
        ("cpu_stat", |f| {
            f.files.borrow_mut().insert(
                CPU_STAT.into(),
                VecDeque::from([Ok("user_usec 1\n".into()), Ok("usage_usec 1\n".into())]),
            );
        }),
        ("cpu_stat", |f| {
            f.files.borrow_mut().insert(
                CPU_STAT.into(),
                VecDeque::from([Ok("usage_usec 1\n".into()), Ok("usage_usec x\n".into())]),
            );
        }),
    ];
    for (field, spoil) in cases {
        let mut f = Fake::new();
        f.scenario(2, 20, 30);
        spoil(&mut f);
        assert_eq!(f.refused(), format!("reading_failed@{field}"));
        assert!(f.sets().is_empty(), "{field}");
    }
}

// spec: specs/ap/quota.md#R9
#[test]
fn r9_the_quota_is_the_limit_less_the_margin_and_the_outside_held_between_floor_and_ceiling() {
    // Two CPUs, busy 75 per cent and the slice at 40: outside 110, 160 - 110 = 50 held to the floor 60.
    let mut f = Fake::new();
    f.scenario(2, 75, 40);
    let answer = f.ok();
    assert_eq!(
        (answer["outside"].clone(), answer["quota"].clone()),
        (json!(110), json!(60))
    );
    // Busy 20 with the slice at 30: outside 10, quota 150.
    let mut f = Fake::new();
    f.scenario(2, 20, 30);
    let answer = f.ok();
    assert_eq!(
        (answer["outside"].clone(), answer["quota"].clone()),
        (json!(10), json!(150))
    );
    // The ceiling: four idle CPUs would leave 320.
    let mut f = Fake::new();
    f.scenario(4, 0, 0);
    assert_eq!(f.ok()["quota"], 170);
    f.policy_is(&json!({"quota": {"slice": SLICE, "ceiling": 400}}));
    f.scenario(4, 0, 0);
    assert_eq!(f.ok()["quota"], 320);
    // The floor and the default floor beneath a lower ceiling.
    f.policy_is(&json!({"quota": {"slice": SLICE, "floor": 25}}));
    f.scenario(2, 100, 0);
    assert_eq!(f.ok()["quota"], 25);
    f.policy_is(&json!({"quota": {"slice": SLICE, "ceiling": 50}}));
    f.scenario(2, 100, 0);
    assert_eq!(f.ok()["quota"], 50);
    // The busy limit and the margin.
    f.policy_is(&json!({"quota": {"slice": SLICE, "busy_limit": 60, "margin": 10}}));
    f.scenario(2, 10, 0);
    assert_eq!(f.ok()["quota"], 80);
}

// spec: specs/ap/quota.md#R10
#[test]
fn r10_it_sets_only_a_value_that_differs_and_reads_it_back() {
    let mut f = Fake::new();
    f.scenario(2, 20, 30);
    f.quota = Some("1.500000s".into());
    let answer = f.ok();
    assert_eq!(
        (
            answer["quota"].clone(),
            answer["before"].clone(),
            answer["set"].clone()
        ),
        (json!(150), json!(150), json!(false))
    );
    assert!(f.sets().is_empty());
    for now in ["600ms", "infinity", "1.510000s"] {
        let mut f = Fake::new();
        f.scenario(2, 20, 30);
        f.quota = Some(now.into());
        let answer = f.ok();
        assert_eq!(
            (answer["quota"].clone(), answer["set"].clone()),
            (json!(150), json!(true)),
            "{now}"
        );
        assert_eq!(
            f.sets(),
            [["set-property", "--runtime", SLICE, "CPUQuota=150%"]]
        );
        // The value is read again after the set, and nothing is called after that.
        assert_eq!(f.calls.last().unwrap()[..2], ["show", "-p"]);
        assert_eq!(f.calls.last().unwrap()[2], "CPUQuotaPerSecUSec");
    }
    // A set that changes nothing: `set_failed` with the value read; a call that fails.
    let mut f = Fake::new();
    f.scenario(2, 20, 30);
    f.quota = Some("600ms".into());
    f.set_works = false;
    let e = f.turn().err().unwrap();
    assert_eq!(
        (e.code, e.field.as_str(), e.detail),
        ("set_failed", "quota", json!(60))
    );
    let mut f = Fake::new();
    f.scenario(2, 20, 30);
    f.set_works = false;
    f.quota = Some("infinity".into());
    assert_eq!(f.turn().err().unwrap().detail, Value::Null);
    let mut f = Fake::new();
    f.scenario(2, 20, 30);
    f.set_call_ok = false;
    assert_eq!(f.refused(), "set_failed@set_property");
}

// spec: specs/ap/quota.md#R11
#[test]
fn r11_a_turn_makes_only_the_three_calls_on_the_one_slice_and_reads_only_its_own_files() {
    let mut f = Fake::new();
    f.scenario(3, 20, 30);
    let answer = f.ok();
    // The fake's CPUs, not this host's: nothing of the real `/proc/stat` was read.
    assert_eq!(answer["cpus"], 3);
    let show = |what: &str| {
        ["show", "-p", what, "--value", SLICE]
            .map(String::from)
            .to_vec()
    };
    assert_eq!(
        f.calls,
        [
            show("ControlGroup"),
            show("ControlGroup"),
            show("CPUQuotaPerSecUSec"),
            ["set-property", "--runtime", SLICE, "CPUQuota=170%"]
                .map(String::from)
                .to_vec(),
            show("CPUQuotaPerSecUSec"),
        ]
    );
    assert_eq!(
        f.host_reads(),
        ["/proc/stat", CPU_STAT, "/proc/stat", CPU_STAT]
    );
    // The reading function refuses a name that is not a slice's, with no call and no read.
    let mut g = Fake::new();
    for name in [
        "../x.slice",
        "x",
        "-x.slice",
        "agents-x.slice",
        "x.slice --user",
    ] {
        assert_eq!(load(&mut g, name).err().unwrap().code, "reading_failed");
    }
    g.nothing_else();
}

// spec: specs/ap/quota.md#R12
#[test]
fn r12_the_answer_is_one_line_with_its_members_and_a_failure_has_its_exit() {
    let mut f = Fake::new();
    f.scenario(2, 20, 30);
    f.quota = Some("600ms".into());
    assert_eq!(
        f.ok(),
        json!({"at": "2026-11-14T09:30:00Z", "slice": SLICE, "cpus": 2, "busy": 20, "slice_use": 30,
               "outside": 10, "before": 60, "quota": 150, "set": true})
    );
    for (code, status) in [
        ("usage", 2),
        ("policy_invalid", 2),
        ("reading_failed", 3),
        ("set_failed", 3),
    ] {
        assert_eq!(exit_status(code), status, "{code}");
    }
}

// spec: specs/ap/quota.md#R13
#[test]
fn r13_the_service_and_the_timer_are_the_ones_the_spec_names() {
    let files = [
        include_str!("../../../deploy/ap-quota.service"),
        include_str!("../../../deploy/ap-quota.timer"),
    ];
    let lines = |text: &str| {
        text.lines()
            .filter(|l| !l.starts_with('#'))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let (service, timer) = (lines(files[0]), lines(files[1]));
    for want in [
        "Type=oneshot",
        "ExecStart=/usr/local/bin/ap quota",
        "Environment=HOME=/root",
        "TimeoutStartSec=90",
    ] {
        assert!(service.iter().any(|l| l == want), "{want}");
    }
    // Not less than the longest a turn can take: every call at its bound (a turn that sets reads the
    // value twice) and the wait between the readings.
    let mut f = Fake::new();
    f.scenario(2, 20, 30);
    f.ok();
    assert_eq!(f.calls.len(), 5);
    let longest = f.bounds.iter().sum::<u64>() + f.waited.as_millis() as u64;
    assert_eq!(longest, 80_000);
    let timeout = service.iter().find_map(|l| l.strip_prefix("TimeoutStartSec="));
    assert!(timeout.unwrap().parse::<u64>().unwrap() * 1000 >= longest);
    // The service runs outside the slice it sets.
    assert!(!service.iter().any(|l| l.starts_with("Slice=")));
    for want in [
        "OnBootSec=1min",
        "OnUnitInactiveSec=20s",
        "AccuracySec=1s",
        "WantedBy=timers.target",
    ] {
        assert!(timer.iter().any(|l| l == want), "{want}");
    }
}
