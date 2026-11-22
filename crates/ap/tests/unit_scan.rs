//! The scan of the added lines for a secret (`specs/ap/unit-run.md` R44): the five forms, the reading
//! of two commits made with `git` in a temporary directory, the admissions and the detail. Every
//! secret-shaped text here is built at run time from parts, so that no line of this file holds a form.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
// spec: specs/ap/unit-run.md#R44
use ap::host::Host;
use ap::unit::scan::{Ended, Hit, added, admissions, detail, form, scan};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A run of `n` letters and digits that holds both.
fn run_of(n: usize) -> String {
    "aB3dE5".chars().cycle().take(n).collect()
}
fn github(n: usize) -> String {
    format!("{}{}_{}", "gh", "p", run_of(n))
}
fn model(n: usize) -> String {
    format!("{}-{}", "sk", run_of(n))
}
fn google(n: usize) -> String {
    format!("{}{}{}", "AI", "za", run_of(n))
}
fn tailnet(n: usize) -> String {
    format!("{}-{}", "tskey", run_of(n))
}
fn header(kind: &str) -> String {
    format!("{}{}{kind}{}", "-----", "BEGIN ", "PRIVATE KEY-----")
}
fn assignment(name: &str, value: &str) -> String {
    format!("{name} = \"{value}\"")
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn each_form_matches_its_line_and_not_its_near_misses() {
    let pat = |n: usize| format!("{}_pat_{}", "github", run_of(n));
    let yes: Vec<(String, &str)> = vec![
        (header(""), "private_key"),
        (header("OPENSSH "), "private_key"),
        (format!("  {}", header("RSA 2 ")), "private_key"),
        (format!("key = {}", github(36)), "github_token"),
        (format!("key = {}", github(40)), "github_token"),
        (github(36), "github_token"),
        (pat(22), "github_token"),
        (format!("x={}", pat(30)), "github_token"),
        (model(32), "model_key"),
        (format!("k: {}-ant-api03-{}", "sk", run_of(40)), "model_key"),
        (google(35), "model_key"),
        (tailnet(20), "tailnet_key"),
        (format!("{}-auth-{}", "tskey", run_of(24)), "tailnet_key"),
        (assignment("token", &run_of(32)), "assigned_value"),
        (assignment("Secret", &run_of(40)), "assigned_value"),
        (format!("export API_KEY:={}", run_of(32)), "assigned_value"),
        (format!("db.password='{}'", run_of(32)), "assigned_value"),
        (format!("api-key\t:\t{}", run_of(32)), "assigned_value"),
        (
            format!("\"private_key\": \"{}\"", run_of(33)),
            "assigned_value",
        ),
        // The first form in the order of the specification.
        (format!("token = {}", github(36)), "github_token"),
        (format!("{} {}", model(32), tailnet(20)), "model_key"),
    ];
    for (line, expected) in &yes {
        assert_eq!(form(line.as_bytes()), Some(*expected), "{expected}");
    }
    let no: Vec<(String, &str)> = vec![
        (header("").replace("BEGIN ", "BEGIN"), "no space"),
        (header("").to_lowercase(), "lower case"),
        (header("").replacen("-----", "----", 1), "four dashes"),
        (
            format!("{}{}", "-----BEGIN ", "CERTIFICATE-----"),
            "a certificate",
        ),
        (github(35), "github, one byte short"),
        (format!("_{}", github(36)), "github, after an underscore"),
        (format!("-{}", github(36)), "github, after a dash"),
        (format!("x{}", github(36)), "github, after a letter"),
        (github(36).to_uppercase(), "github, upper case"),
        (
            format!("{}{}_{}", "gh", "x", run_of(36)),
            "github, another letter",
        ),
        (pat(21), "pat, one byte short"),
        (format!("a{}", pat(30)), "pat, after a letter"),
        (model(31), "model, one byte short"),
        (format!("x-{}", model(32)), "model, after a dash"),
        (format!("_{}", model(32)), "model, after an underscore"),
        (model(32).to_uppercase(), "model, upper case"),
        (google(34), "google, one byte short"),
        (google(35).to_lowercase(), "google, lower case"),
        (format!("1{}", google(35)), "google, after a digit"),
        (tailnet(19), "tailnet, one byte short"),
        (tailnet(20).to_uppercase(), "tailnet, upper case"),
        (format!("x{}", tailnet(20)), "tailnet, after a letter"),
        (assignment("token", &run_of(31)), "value, 31 bytes"),
        (
            assignment("token", &"a".repeat(32)),
            "value without a digit",
        ),
        (
            assignment("token", &"7".repeat(32)),
            "value without a letter",
        ),
        (format!("token {}", run_of(32)), "no separator"),
        (
            assignment("author", &run_of(40)),
            "a name that is not listed",
        ),
        (
            format!("token = a b{}", run_of(31)),
            "a value that stops at a space",
        ),
    ];
    for (line, what) in &no {
        assert_eq!(form(line.as_bytes()), None, "{what}");
    }
    // A line that is not UTF-8 is scanned by its bytes.
    let mut bytes = vec![0xff, 0xfe, b' '];
    bytes.extend(github(36).bytes());
    assert_eq!(form(&bytes), Some("github_token"));
}

#[test]
fn the_added_lines_of_a_reply_are_read_from_its_first_hunk_header() {
    let mut reply: Vec<u8> = b"diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n".to_vec();
    reply.extend(b"+++ not read before a hunk\n");
    reply.extend(b"@@ -3,2 +3,0 @@ context \xff\n-gone\n-gone\n");
    reply.extend(b"@@ -7 +5,3 @@\n+one\n-old\n+++two\n+three \r\n\\ No newline at end of file\n");
    let lines = added(&reply).unwrap();
    let want: Vec<(u64, &[u8])> = vec![(5, b"one"), (6, b"++two"), (7, b"three \r")];
    assert_eq!(lines, want);
    assert_eq!(added(b"Binary files a/x and b/x differ\n").unwrap(), vec![]);
    assert!(added(b"@@ nonsense @@\n+x\n").is_none());
    // A type change is two sections of one reply: the second one's header lines are no lines.
    let name = github(36);
    let change = format!(
        "diff --git a/{name} b/{name}\ndeleted file mode 100644\n--- a/{name}\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-b\ndiff --git a/{name} b/{name}\nnew file mode 120000\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1 @@\n+none\n\\ No newline at end of file\n"
    );
    assert_eq!(added(change.as_bytes()).unwrap(), vec![(1, &b"none"[..])]);
    // A hunk that holds fewer lines than its header counts is not read.
    assert!(added(b"@@ -0,0 +1,2 @@\n+one\n").is_none());
}

#[test]
fn a_type_change_reads_no_header_line_of_the_diff() {
    let r = Repo::new();
    let name = github(36);
    r.lines(&name, &["plain", "text"]);
    let base = r.commit();
    std::fs::remove_file(r.0.join(&name)).unwrap();
    std::os::unix::fs::symlink("none", r.0.join(&name)).unwrap();
    let head = r.commit();
    assert!(r.scan(&base, &head, "").is_ok());
}

#[test]
fn only_exact_admission_lines_admit() {
    let digit = "0123456789abcdef".repeat(4);
    let notes = format!(
        "Free text.\nAdmitted line: a/b c.txt {digit}\nAdmitted line: x {digit} \nAdmitted line: y {}\nadmitted line: z {digit}\nAdmitted line:  {digit}\nAdmitted line: w {}\nAdmitted line: v {digit}\r\nAdmitted line: u {digit}",
        &digit[..63],
        digit.to_uppercase(),
    );
    let found = admissions(&notes);
    let mut want = vec![
        ("a/b c.txt".to_owned(), digit.clone()),
        ("u".to_owned(), digit),
    ];
    want.sort();
    let mut found: Vec<_> = found.into_iter().collect();
    found.sort();
    assert_eq!(found, want);
}

/// A scratch repository in a temporary directory.
struct Repo(PathBuf);
static COUNT: AtomicUsize = AtomicUsize::new(0);
impl Repo {
    fn new() -> Repo {
        let n = COUNT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ap-scan-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let r = Repo(dir);
        r.git(&["init", "--quiet", "-b", "main"]);
        r
    }
    fn git(&self, words: &[&str]) -> String {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=invented",
                "-c",
                "user.email=invented@example.invalid",
            ])
            .args(["-c", "commit.gpgsign=false", "-C"])
            .arg(&self.0)
            .args(words)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {words:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap().trim_end().to_owned()
    }
    fn put(&self, rel: &str, bytes: &[u8]) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    fn lines(&self, rel: &str, lines: &[&str]) {
        self.put(rel, (lines.join("\n") + "\n").as_bytes());
    }
    fn commit(&self) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "--quiet", "--allow-empty", "-m", "invented"]);
        self.git(&["rev-parse", "HEAD"])
    }
    fn scan(&self, base: &str, head: &str, notes: &str) -> Result<(), Ended> {
        scan(&mut Host, self.0.to_str().unwrap(), base, head, notes)
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
/// The hits of a stop: (path, line, form), and the count.
fn hits(ended: &Ended) -> (Vec<(String, u64, String)>, u64) {
    assert_eq!(ended.cause, "secret_in_diff", "{ended:?}");
    let v: Value = serde_json::from_str(&ended.detail).unwrap();
    let lines = v["lines"].as_array().unwrap();
    let lines = lines
        .iter()
        .map(|l| {
            (
                l["path"].as_str().unwrap().to_owned(),
                l["line"].as_u64().unwrap(),
                l["form"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    (lines, v["count"].as_u64().unwrap())
}
fn want(list: &[(&str, u64, &str)]) -> (Vec<(String, u64, String)>, u64) {
    let lines = list
        .iter()
        .map(|(p, n, f)| ((*p).to_owned(), *n, (*f).to_owned()))
        .collect();
    (lines, list.len() as u64)
}

/// A base with an unchanged file and one head that adds hits at several kinds of path.
fn with_hits() -> (Repo, String, String, String) {
    let r = Repo::new();
    let token = github(36);
    r.lines("keep.txt", &["one", "two", "three", "four", "five"]);
    let base = r.commit();
    r.lines("dir/sub/a.txt", &[&token, "tail"]);
    r.lines("keep.txt", &["one", "two", "three", "four", "five", &token]);
    r.lines("star*.txt", &[&token]);
    r.lines("starX.txt", &["none", &token]);
    r.lines("with space.txt", &["first", &token]);
    r.lines("ünï.txt", &["a", "b", &token]);
    let head = r.commit();
    (r, base, head, token)
}

#[test]
fn a_hit_is_named_by_its_path_and_line_number() {
    let (r, base, head, _) = with_hits();
    let ended = r.scan(&base, &head, "").unwrap_err();
    assert_eq!(
        hits(&ended),
        want(&[
            ("dir/sub/a.txt", 1, "github_token"),
            ("keep.txt", 6, "github_token"),
            ("star*.txt", 1, "github_token"),
            ("starX.txt", 2, "github_token"),
            ("with space.txt", 2, "github_token"),
            ("ünï.txt", 3, "github_token"),
        ])
    );
}

#[test]
fn the_repository_configuration_changes_no_path_and_no_line_number() {
    let r = Repo::new();
    let (a, b) = (github(36), model(32));
    r.lines("f.txt", &["one", "two", "three", "four", "five", "six"]);
    let base = r.commit();
    r.lines("f.txt", &["one", &a, "three", &b, "five", "six", &a]);
    r.lines("sub/ü.txt", &[&a]);
    let head = r.commit();
    let expected = want(&[
        ("f.txt", 2, "github_token"),
        ("f.txt", 4, "model_key"),
        ("f.txt", 7, "github_token"),
        ("sub/ü.txt", 1, "github_token"),
    ]);
    assert_eq!(hits(&r.scan(&base, &head, "").unwrap_err()), expected);
    for (key, value) in [
        ("diff.noprefix", "true"),
        ("diff.mnemonicPrefix", "true"),
        ("core.quotePath", "false"),
        ("diff.interHunkContext", "5"),
    ] {
        r.git(&["config", key, value]);
        assert_eq!(
            hits(&r.scan(&base, &head, "").unwrap_err()),
            expected,
            "{key}"
        );
    }
}

#[test]
fn deleted_lines_deleted_files_and_binary_files_give_no_hit() {
    let r = Repo::new();
    let token = github(36);
    r.lines("edit.txt", &["one", &token, "three"]);
    r.lines("gone.txt", &[&token]);
    let base = r.commit();
    r.lines("edit.txt", &["one", "three"]);
    std::fs::remove_file(r.0.join("gone.txt")).unwrap();
    let mut binary = vec![0u8, 1, 2, b'\n'];
    binary.extend(token.bytes());
    r.put("blob.bin", &binary);
    let head = r.commit();
    assert!(r.scan(&base, &head, "").is_ok());
}

#[test]
fn a_file_that_is_not_utf_8_is_read() {
    let r = Repo::new();
    let base = r.commit();
    let mut bytes = vec![0xff, 0xfe, b'\n'];
    bytes.extend(format!("{} \u{ff}\n", github(36)).bytes());
    bytes.extend([0xff, b'\n']);
    r.put("latin.txt", &bytes);
    let head = r.commit();
    assert_eq!(
        hits(&r.scan(&base, &head, "").unwrap_err()),
        want(&[("latin.txt", 2, "github_token")])
    );
    // The same file without a hit is no failure.
    let r = Repo::new();
    let base = r.commit();
    r.put("latin.txt", &[0xff, 0xfe, b'\n', 0xfd, b'\n']);
    let head = r.commit();
    assert!(r.scan(&base, &head, "").is_ok());
}

#[test]
fn an_attribute_file_the_head_adds_hides_no_line() {
    let r = Repo::new();
    let base = r.commit();
    r.put(".gitattributes", b"* -diff\n");
    r.lines("hidden.txt", &["a", &github(36)]);
    let head = r.commit();
    assert_eq!(
        hits(&r.scan(&base, &head, "").unwrap_err()),
        want(&[("hidden.txt", 2, "github_token")])
    );
}

#[test]
fn a_moved_file_has_its_hits_at_the_new_path() {
    let r = Repo::new();
    r.lines("old/x.txt", &["a", &github(36), "b", "c"]);
    let base = r.commit();
    r.git(&["mv", "old/x.txt", "new.txt"]);
    let head = r.commit();
    assert_eq!(
        hits(&r.scan(&base, &head, "").unwrap_err()),
        want(&[("new.txt", 2, "github_token")])
    );
}

#[test]
fn a_diff_past_the_ports_limit_ends_without_a_reply_and_names_its_path() {
    let r = Repo::new();
    let base = r.commit();
    let line = "x".repeat(99) + "\n";
    r.put("small.txt", b"ok\n");
    r.put("big dir/huge.txt", line.repeat(10_700).as_bytes());
    let head = r.commit();
    assert!(line.len() * 10_700 > 1_048_576);
    let ended = r.scan(&base, &head, "").unwrap_err();
    assert_eq!(ended.cause, "diff_unread");
    assert_eq!(
        serde_json::from_str::<Value>(&ended.detail).unwrap(),
        serde_json::json!({"path": "big dir/huge.txt"})
    );
}

#[test]
fn a_failed_list_ends_git_failed() {
    let r = Repo::new();
    let head = r.commit();
    let ended = r.scan(&head, &"0".repeat(40), "").unwrap_err();
    assert_eq!(ended.cause, "git_failed");
}

#[test]
fn an_admission_takes_that_line_at_that_path_only() {
    let r = Repo::new();
    let base = r.commit();
    let token = github(36);
    r.lines("fx dir/fixture.txt", &["first", &token]);
    r.lines("other.txt", &[&token]);
    let head = r.commit();
    let digest = sha(token.as_bytes());
    let admit = |path: &str, digest: &str| format!("Intro\nAdmitted line: {path} {digest}\n");
    // The admitted path and digest admit that line only: the other path is a hit.
    let ended = r
        .scan(&base, &head, &admit("fx dir/fixture.txt", &digest))
        .unwrap_err();
    assert_eq!(hits(&ended), want(&[("other.txt", 1, "github_token")]));
    let both = admit("fx dir/fixture.txt", &digest) + &admit("other.txt", &digest);
    assert!(r.scan(&base, &head, &both).is_ok());
    // Another digest, a digest in upper case, a trailing space and another path admit nothing.
    for notes in [
        admit("fx dir/fixture.txt", &sha(b"another")) + &admit("other.txt", &sha(b"another")),
        admit("fx dir/fixture.txt", &digest.to_uppercase())
            + &admit("other.txt", &digest.to_uppercase()),
        format!(
            "Admitted line: fx dir/fixture.txt {digest} \nAdmitted line: other.txt {digest} \n"
        ),
        admit("nothing.txt", &digest),
    ] {
        let ended = r.scan(&base, &head, &notes).unwrap_err();
        assert_eq!(hits(&ended).1, 2, "{notes}");
    }
}

#[test]
fn an_admission_counts_the_carriage_return_and_the_last_space_of_a_line() {
    let r = Repo::new();
    let base = r.commit();
    let token = github(36);
    r.put("crlf.txt", format!("{token}\r\n").as_bytes());
    r.put("space.txt", format!("{token} \n").as_bytes());
    let head = r.commit();
    let notes = format!(
        "Admitted line: crlf.txt {}\nAdmitted line: space.txt {}\n",
        sha(format!("{token}\r").as_bytes()),
        sha(format!("{token} ").as_bytes())
    );
    assert!(r.scan(&base, &head, &notes).is_ok());
    let wrong = format!("Admitted line: crlf.txt {}\n", sha(token.as_bytes()));
    assert_eq!(hits(&r.scan(&base, &head, &wrong).unwrap_err()).1, 2);
}

#[test]
fn the_detail_holds_no_piece_of_a_hit_line() {
    let r = Repo::new();
    let base = r.commit();
    let value = format!("{}{}", "Zq9Xw8Ev7Ru6Ty5Ui4Op3As2Df1Gh0Jk", "LmNbVcXz");
    let line = assignment("password", &value);
    r.lines("conf.txt", &["x", &line]);
    let head = r.commit();
    let ended = r.scan(&base, &head, "").unwrap_err();
    assert_eq!(hits(&ended), want(&[("conf.txt", 2, "assigned_value")]));
    for piece in value.as_bytes().windows(8) {
        let piece = std::str::from_utf8(piece).unwrap();
        assert!(!ended.detail.contains(piece), "{piece}");
    }
    assert!(!ended.detail.contains(&sha(line.as_bytes())));
}

#[test]
fn twelve_hits_on_long_paths_keep_the_detail_within_a_thousand_bytes() {
    let r = Repo::new();
    let base = r.commit();
    let token = github(36);
    for i in 0..12 {
        r.lines(&format!("{}{i:02}.txt", "p".repeat(144)), &[&token]);
    }
    let head = r.commit();
    let ended = r.scan(&base, &head, "").unwrap_err();
    assert!(ended.detail.len() <= 1000, "{}", ended.detail.len());
    let (lines, count) = hits(&ended);
    assert_eq!(count, 12);
    assert!(!lines.is_empty() && lines.len() < 12, "{}", lines.len());
    for (i, (path, line, _)) in lines.iter().enumerate() {
        assert_eq!((path.len(), *line), (150, 1));
        assert_eq!(path, &format!("{}{i:02}.txt", "p".repeat(144)));
    }
    // The detail's own bound: ten hits at most, and every hit left out comes after the kept ones.
    let hit = |i: usize| Hit {
        path: format!("p{i}"),
        line: 1,
        form: "github_token",
    };
    let many: Vec<Hit> = (0..15).map(hit).collect();
    let v: Value = serde_json::from_str(&detail(&many, 15)).unwrap();
    assert_eq!(
        (v["lines"].as_array().unwrap().len(), v["count"].as_u64()),
        (10, Some(15))
    );
}
