//! The page's block and `ap accept` (README "The page's block"): the binary runs in a scratch Git
//! repository of its own under a temporary directory, with a page file beside it; `origin/main` is a
//! ref of that repository. No test reads or writes this repository's Git state.
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU32, Ordering},
};

static NEXT: AtomicU32 = AtomicU32::new(0);

struct S {
    root: PathBuf,
}
fn git(dir: &PathBuf, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@e"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
impl S {
    /// A repository whose `origin/main` is its first commit: `README.md`, `src/a.rs` (three lines,
    /// one of them `old_word`) and `docs/d.md`.
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "ap-page-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("repo")).unwrap();
        std::fs::create_dir_all(root.join("pages")).unwrap();
        let s = Self { root };
        git(&s.repo(), &["-c", "init.defaultBranch=main", "init", "-q"]);
        s.put("README.md", b"base\n");
        s.put("src/a.rs", b"one\nold_word\nthree\n");
        s.put("docs/d.md", b"doc\n");
        s.commit("base");
        git(
            &s.repo(),
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
        );
        s
    }
    fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }
    fn put(&self, path: &str, bytes: &[u8]) {
        let p = self.repo().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    fn commit(&self, message: &str) {
        git(&self.repo(), &["add", "-A"]);
        git(&self.repo(), &["commit", "-qm", message]);
    }
    /// A commit that changes `src/a.rs` (one line replaced: 2 changed lines), adds `src/n.rs` (one
    /// line) and `src/b.bin` (binary) and removes `docs/d.md` (one line).
    fn change(&self) {
        self.put("src/a.rs", b"one\nnew_word\nthree\n");
        self.put("src/n.rs", b"fn n() {}\n");
        self.put("src/b.bin", b"bin\0ary\0new_word\0");
        std::fs::remove_file(self.repo().join("docs/d.md")).unwrap();
        self.commit("change");
    }
    fn page(&self, name: &str, text: &str) -> String {
        let p = self.root.join("pages").join(name);
        std::fs::write(&p, text).unwrap();
        p.to_str().unwrap().to_owned()
    }
    fn block(&self, block: &Value) -> String {
        self.page("p.md", &format!("# T\n```unit\n{block}\n```\n"))
    }
    /// `ap accept <page>` in the repository: exit status, stdout lines, stderr.
    fn accept(&self, args: &[&str]) -> (i32, Vec<String>, String) {
        run_in(&self.repo(), args)
    }
    fn lines(&self, block: &Value) -> Vec<String> {
        let page = self.block(block);
        let (code, out, err) = self.accept(&[&page]);
        assert_eq!(code, i32::from(out != ["accept: ok"]), "{out:?} {err}");
        assert!(err.is_empty(), "{err}");
        out
    }
}
fn run_in(dir: &PathBuf, args: &[&str]) -> (i32, Vec<String>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ap"))
        .arg("accept")
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let lines = String::from_utf8(out.stdout).unwrap();
    (
        out.status.code().unwrap(),
        lines.lines().map(str::to_owned).collect(),
        String::from_utf8(out.stderr).unwrap(),
    )
}
fn unit(write_set: &[&str], checks: Value) -> Value {
    json!({"unit": "T-1", "write_set": write_set, "checks": checks})
}
/// The refusal of a run: its code, field and detail.
fn refusal((code, out, err): (i32, Vec<String>, String)) -> (String, String, Value) {
    assert_eq!((code, out), (2, vec![]), "{err}");
    let e: Value = serde_json::from_str(&err).unwrap();
    let s = |k: &str| e["error"][k].as_str().unwrap_or_default().to_owned();
    (s("code"), s("field"), e["error"]["detail"].clone())
}
fn failed(lines: &[String], kind: &str) -> usize {
    let prefix = format!("accept: FAILED: {kind}: ");
    lines.iter().filter(|l| l.starts_with(&prefix)).count()
}

#[test]
fn a1_a_passing_block_prints_ok_and_writes_nothing() {
    let s = S::new();
    s.change();
    let checks = json!({
        "unchanged": ["README.md", "docs/**/x"],
        "adds": [{"path": "src/a.rs", "words": ["new_word"]},
                 {"path": "src/**", "words": ["fn n()", "new_word", "one"]}],
        "holds": [{"path": "src/n.rs", "text": "fn n"},
                  {"path": "src/n.rs", "exact": "fn n() {}\n"}],
        "lines": [{"path": "src/a.rs", "max": 2, "min": 2}, {"path": "src/**", "max": 3}]
    });
    // `one` is no added line of this change: the check fails, and only that word.
    let lines = s.lines(&unit(&["src/**", "docs/d.md"], checks.clone()));
    assert_eq!(
        lines,
        [r#"accept: FAILED: adds: "src/**": no added line contains "one""#]
    );
    let mut ok = checks;
    ok["adds"][1]["words"] = json!(["fn n()", "new_word"]);
    let before = git(&s.repo(), &["status", "--porcelain=v2", "--branch"]);
    let tree = std::fs::read_dir(s.root.join("pages")).unwrap().count();
    assert_eq!(s.lines(&unit(&["src/**", "docs/d.md"], ok)), ["accept: ok"]);
    assert_eq!(
        git(&s.repo(), &["status", "--porcelain=v2", "--branch"]),
        before
    );
    assert_eq!(
        std::fs::read_dir(s.root.join("pages")).unwrap().count(),
        tree
    );
    // A block without `checks` holds the write set only.
    let page = s.block(&json!({"unit": "T-1", "write_set": ["src/**", "docs/d.md"]}));
    assert_eq!(s.accept(&[&page]).1, ["accept: ok"]);
}

#[test]
fn a2_a_dirty_worktree_and_a_path_outside_the_write_set_are_printed_in_order() {
    let s = S::new();
    s.change();
    s.put("stray.txt", b"x");
    let checks = json!({"unchanged": ["docs/d.md"], "lines": [{"path": "src/**", "max": 0}]});
    let lines = s.lines(&unit(&["src/a.rs"], checks));
    let kinds: Vec<_> = lines
        .iter()
        .map(|l| l.split(": ").nth(2).unwrap())
        .collect();
    assert_eq!(kinds, ["clean", "write_set", "unchanged", "lines"]);
    assert!(lines[0].starts_with("accept: FAILED: clean: "));
    assert!(lines[1].starts_with("accept: FAILED: write_set: 3 paths outside"));
    assert!(lines[1].contains("docs/d.md") && lines[1].contains("src/n.rs"));
    assert!(!lines[1].contains("src/a.rs"));
}

#[test]
fn a3_unchanged_matches_a_directory_by_its_descendants_only() {
    let s = S::new();
    s.change();
    let w = ["src/**", "docs/**"];
    // Each failure names the pattern: the exact path, a directory, and a name with the same prefix.
    let one = |p: &str| s.lines(&unit(&w, json!({"unchanged": [p]})));
    assert_eq!(failed(&one("docs/d.md"), "unchanged"), 1);
    assert_eq!(failed(&one("docs/**"), "unchanged"), 1);
    assert_eq!(failed(&one("src/**"), "unchanged"), 1);
    // The directory itself is no changed path; another name with the prefix is not a descendant.
    assert_eq!(one("docs"), ["accept: ok"]);
    assert_eq!(one("sr/**"), ["accept: ok"]);
    assert_eq!(one("README.md"), ["accept: ok"]);
}

#[test]
fn a4_adds_judges_added_lines_of_the_matched_paths_only() {
    let s = S::new();
    s.change();
    let w = ["src/**", "docs/**"];
    let adds = |p: &str, words: &[&str]| {
        s.lines(&unit(&w, json!({"adds": [{"path": p, "words": words}]})))
    };
    assert_eq!(adds("src/a.rs", &["new_word"]), ["accept: ok"]);
    // A word only in a removed line.
    assert_eq!(
        adds("src/a.rs", &["old_word"]),
        [r#"accept: FAILED: adds: "src/a.rs": no added line contains "old_word""#]
    );
    // A word in an added line of another file, and of a path the pattern does not match.
    assert_eq!(failed(&adds("src/a.rs", &["fn n()"]), "adds"), 1);
    assert_eq!(adds("src/**", &["fn n()"]), ["accept: ok"]);
    assert_eq!(failed(&adds("src/n.rs", &["new_word"]), "adds"), 1);
    // One line per missing word; a word that spans an added line's start is literal text.
    assert_eq!(
        failed(&adds("src/a.rs", &["new_word", "x", "y"]), "adds"),
        2
    );
    assert_eq!(adds("src/a.rs", &["w_wo", "new_"]), ["accept: ok"]);
    assert_eq!(failed(&adds("src/a.rs", &["new.word"]), "adds"), 1);
    // The line `+++ b/…` of the diff's header is no added line, and a pattern that matches no
    // changed path fails every word.
    assert_eq!(failed(&adds("src/a.rs", &["b/src/a.rs"]), "adds"), 1);
    assert_eq!(failed(&adds("nothing/**", &["new_word"]), "adds"), 1);
    // A deleted file has no added line; an added line that starts with `++` is one.
    assert_eq!(failed(&adds("docs/d.md", &["doc"]), "adds"), 1);
    s.put("src/p.rs", b"++ plus\n");
    s.commit("plus");
    assert_eq!(adds("src/p.rs", &["++ plus"]), ["accept: ok"]);
    assert_eq!(
        adds("src/p.rs", &["+++ plus"]),
        [r#"accept: FAILED: adds: "src/p.rs": no added line contains "+++ plus""#]
    );
}

#[test]
fn a5_holds_reads_the_commit_head_as_bytes() {
    let s = S::new();
    s.change();
    let w = ["src/**", "docs/**"];
    let holds = |h: Value| s.lines(&unit(&w, json!({"holds": [h]})));
    let ok = |h: Value| assert_eq!(holds(h), ["accept: ok"]);
    let no = |h: Value| assert_eq!(failed(&holds(h), "holds"), 1);
    ok(json!({"path": "src/n.rs", "text": "n() {"}));
    no(json!({"path": "src/n.rs", "text": "m() {"}));
    ok(json!({"path": "src/n.rs", "exact": "fn n() {}\n"}));
    // An exact text that differs by a trailing line feed, or by the lack of one.
    no(json!({"path": "src/n.rs", "exact": "fn n() {}"}));
    no(json!({"path": "src/n.rs", "exact": "fn n() {}\n\n"}));
    // The file's bytes are not UTF-8: the text is found in them, the exact text is not.
    ok(json!({"path": "src/b.bin", "text": "new_word"}));
    no(json!({"path": "src/b.bin", "exact": "bin"}));
    // Not a regular file of the commit: missing, a directory, a deleted file.
    no(json!({"path": "src/missing.rs", "text": "x"}));
    no(json!({"path": "src", "text": "x"}));
    no(json!({"path": "docs/d.md", "text": "doc"}));
    // A symbolic link.
    std::os::unix::fs::symlink("n.rs", s.repo().join("src/link.rs")).unwrap();
    s.commit("link");
    // The link's own bytes are its target `n.rs`: they would hold the text and be the exact text.
    no(json!({"path": "src/link.rs", "text": "n.rs"}));
    no(json!({"path": "src/link.rs", "exact": "n.rs"}));
    // The working tree is not read: an uncommitted file changes nothing of the answer for HEAD
    // (the dirty tree is its own line, `clean`).
    s.put("src/n.rs", b"changed\n");
    let lines = holds(json!({"path": "src/n.rs", "exact": "fn n() {}\n"}));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("accept: FAILED: clean: "));
    s.put("src/uncommitted.rs", b"text\n");
    assert_eq!(
        failed(
            &holds(json!({"path": "src/uncommitted.rs", "text": "text"})),
            "holds"
        ),
        1
    );
}

#[test]
fn a6_lines_counts_added_plus_removed_at_its_bounds() {
    let s = S::new();
    s.change();
    let w = ["src/**", "docs/**"];
    let lines = |p: &str, max: u64, min: Option<u64>| {
        let mut e = json!({"path": p, "max": max});
        if let Some(min) = min {
            e["min"] = json!(min);
        }
        s.lines(&unit(&w, json!({"lines": [e]})))
    };
    // src/a.rs: one removed and one added line.
    assert_eq!(lines("src/a.rs", 2, Some(2)), ["accept: ok"]);
    assert_eq!(lines("src/a.rs", 2, None), ["accept: ok"]);
    assert_eq!(
        lines("src/a.rs", 1, None),
        [r#"accept: FAILED: lines: "src/a.rs": 2 changed lines, above max 1"#]
    );
    assert_eq!(
        lines("src/a.rs", 5, Some(3)),
        [r#"accept: FAILED: lines: "src/a.rs": 2 changed lines, below min 3"#]
    );
    // All of src/: 2 + 1 (src/n.rs) and a binary file that counts as no line.
    assert_eq!(lines("src/**", 3, Some(3)), ["accept: ok"]);
    assert_eq!(failed(&lines("src/**", 2, None), "lines"), 1);
    assert_eq!(failed(&lines("src/**", 9, Some(4)), "lines"), 1);
    // The binary file alone: a changed path with no line.
    assert_eq!(lines("src/b.bin", 0, Some(0)), ["accept: ok"]);
    // A removed file counts its removed lines; a pattern that matches nothing counts 0.
    assert_eq!(lines("docs/d.md", 1, Some(1)), ["accept: ok"]);
    assert_eq!(lines("nothing", 0, None), ["accept: ok"]);
    assert_eq!(failed(&lines("nothing", 5, Some(1)), "lines"), 1);
    // A binary file has no added line.
    let adds = json!({"adds": [{"path": "src/b.bin", "words": ["new_word"]}]});
    assert_eq!(failed(&s.lines(&unit(&w, adds)), "adds"), 1);
    let unchanged = s.lines(&unit(&w, json!({"unchanged": ["src/b.bin"]})));
    assert_eq!(failed(&unchanged, "unchanged"), 1);
}

#[test]
fn a7_every_failed_check_is_printed_in_one_run() {
    let s = S::new();
    s.change();
    let checks = json!({
        "lines": [{"path": "src/**", "max": 0}],
        "holds": [{"path": "nothing", "text": "x"}],
        "adds": [{"path": "src/a.rs", "words": ["nope"]}],
        "unchanged": ["src/a.rs"]
    });
    let lines = s.lines(&unit(&["src/**"], checks));
    let kinds: Vec<_> = lines
        .iter()
        .map(|l| l.split(": ").nth(2).unwrap())
        .collect();
    assert_eq!(kinds, ["write_set", "unchanged", "adds", "holds", "lines"]);
}

#[test]
fn a8_the_command_refuses_before_writing_anything() {
    let s = S::new();
    s.change();
    let page = s.block(&unit(&["src/**"], json!({})));
    let r = |args: &[&str]| refusal(s.accept(args));
    assert_eq!(r(&[]).0, "usage");
    assert_eq!(r(&[&page, &page]).0, "usage");
    assert_eq!(r(&["p.md"]).0, "usage");
    assert_eq!(r(&[&format!("{page}.gone")]).0, "page_unreadable");
    let none = s.page("none.md", "# T\nWrite set: src/**\n");
    let (code, field, detail) = r(&[&none]);
    assert_eq!(
        (code.as_str(), field.as_str()),
        ("page_block_invalid", "page")
    );
    assert_eq!(detail["reason"], "no_block");
    let twice = s.page(
        "twice.md",
        &format!(
            "Write set: src/**\n```unit\n{}\n```\n",
            unit(&["a"], json!({}))
        ),
    );
    assert_eq!(r(&[&twice]).0, "write_set_twice");
    // Not a worktree: a directory outside every repository; and no `origin/main`.
    let bare = s.root.join("pages");
    let (code, ..) = refusal(run_in(&bare, &[&page]));
    assert_eq!(code, "not_a_worktree");
    git(&s.repo(), &["update-ref", "-d", "refs/remotes/origin/main"]);
    assert_eq!(r(&[&page]).0, "origin_main_unknown");
}

/// `ap accept` on a page of `text` in a directory that is no repository: a block is judged first.
fn refused(text: &str) -> (String, Value) {
    let dir = std::env::temp_dir().join(format!(
        "ap-page-lone-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("bad.md");
    std::fs::write(&page, text).unwrap();
    let (code, field, detail) = refusal(run_in(&dir, &[page.to_str().unwrap()]));
    assert_eq!(field, "page");
    (code, detail)
}
fn bad(block: &Value, reason: &str, member: Option<&str>) {
    let (code, detail) = refused(&format!("```unit\n{block}\n```\n"));
    assert_eq!(code, "page_block_invalid", "{block}");
    assert_eq!(
        detail,
        json!({"reason": reason, "member": member}),
        "{block}"
    );
}

#[test]
fn b1_a_block_that_is_not_one_object_is_refused_with_its_reason() {
    let ok = unit(&["a"], json!({}));
    let two = format!("```unit\n{ok}\n```\n\n```unit\n{ok}\n```\n");
    assert_eq!(refused(&two).1["reason"], "two_blocks");
    // Two opening lines, also when the second is inside the first.
    let inside = format!("```unit\n{ok}\n```unit\n```\n");
    assert_eq!(refused(&inside).1["reason"], "two_blocks");
    assert_eq!(refused(&format!("```unit\n{ok}\n")).1["reason"], "unclosed");
    for body in [
        "[]",
        "\"x\"",
        "7",
        "null",
        "not json",
        "",
        "{\"unit\": \"a\"} {}",
    ] {
        let (code, detail) = refused(&format!("```unit\n{body}\n```\n"));
        assert_eq!(code, "page_block_invalid");
        assert_eq!(detail["reason"], "not_an_object", "{body}");
    }
    // Only an exact line opens or closes: the lines `` ```unit `` with a suffix, with a space in
    // front or in `rust` are no block.
    for text in [
        "```unit \n{}\n```\n",
        " ```unit\n{}\n```\n",
        "```rust\n{}\n```\n",
    ] {
        assert_eq!(
            refused(&format!("{text}Write set: a\n")).1["reason"],
            "no_block",
            "{text}"
        );
    }
}

#[test]
fn b2_an_unknown_member_or_a_member_of_the_wrong_shape_is_named() {
    let ok = || unit(&["a"], json!({}));
    let with = |k: &str, v: Value| {
        let mut b = ok();
        b[k] = v;
        b
    };
    let mut extra = ok();
    extra["bogus"] = json!(1);
    bad(&extra, "unknown_member", Some("bogus"));
    bad(
        &with("checks", json!({"regex": []})),
        "unknown_member",
        Some("checks.regex"),
    );
    let long = "x".repeat(41);
    let patterns = vec!["a"; 41];
    let many = |v: Value| json!(vec![v; 41]);
    for (k, v) in [
        ("unit", json!("")),
        ("unit", json!("a b")),
        ("unit", json!("é")),
        ("unit", json!(long.clone())),
        ("unit", json!(7)),
        ("write_set", json!([])),
        ("write_set", json!(patterns)),
        ("write_set", json!("a")),
        ("write_set", json!([""])),
        ("write_set", json!(["a,b"])),
        ("write_set", json!([" a"])),
        ("write_set", json!(["a "])),
        ("write_set", json!([1])),
        ("checks", json!([])),
        ("repository", json!("owner")),
        ("repository", json!("a/b/c")),
        ("repository", json!(7)),
        ("branch", json!("-x")),
        ("branch", json!("a..b")),
        ("item", json!("")),
        ("item", json!("x".repeat(61))),
        ("tier", json!("")),
        ("tier", json!(long.clone())),
        ("merge", json!("yes")),
    ] {
        bad(&with(k, v), "shape", Some(k));
    }
    let path = "a/b";
    let shaped = |kind: &str, v: Value| {
        let mut b = ok();
        b["checks"] = json!({ kind: v });
        bad(&b, "shape", Some(&format!("checks.{kind}")));
    };
    let w = "w".repeat(2001);
    shaped("unchanged", json!("a"));
    shaped("unchanged", json!([1]));
    shaped("unchanged", json!([""]));
    shaped("unchanged", json!(["a,b"]));
    shaped("unchanged", many(json!("a")));
    shaped("adds", json!([{"path": path}]));
    shaped("adds", json!([{"words": ["w"]}]));
    shaped("adds", json!([{"path": path, "words": []}]));
    shaped("adds", json!([{"path": path, "words": [""]}]));
    shaped("adds", json!([{"path": path, "words": ["a\nb"]}]));
    shaped("adds", json!([{"path": path, "words": [w]}]));
    shaped("adds", json!([{"path": path, "words": ["w"], "extra": 1}]));
    shaped("adds", many(json!({"path": path, "words": ["w"]})));
    shaped("holds", json!([{"path": path}]));
    shaped("holds", json!([{"path": path, "text": "a", "exact": "a"}]));
    shaped("holds", json!([{"path": path, "text": "a\nb"}]));
    shaped("holds", json!([{"path": path, "text": w}]));
    shaped(
        "holds",
        json!([{"path": path, "exact": "x".repeat(65_537)}]),
    );
    shaped("holds", json!([{"path": "../x", "text": "a"}]));
    shaped("holds", json!([{"path": "a//b", "text": "a"}]));
    shaped("holds", json!([{"path": "src/", "text": "a"}]));
    shaped("holds", json!([{"path": "/a", "text": "a"}]));
    shaped("holds", many(json!({"path": path, "text": "a"})));
    shaped("lines", json!([{"path": path}]));
    shaped("lines", json!([{"path": path, "max": -1}]));
    shaped("lines", json!([{"path": path, "max": 1.5}]));
    shaped("lines", json!([{"path": path, "max": "1"}]));
    shaped("lines", json!([{"path": path, "max": 1, "min": 2}]));
    shaped("lines", json!([{"path": path, "max": 1, "min": -1}]));
    shaped("lines", json!([{"path": path, "max": 1, "bound": 1}]));
    shaped("lines", many(json!({"path": path, "max": 1})));
    // A missing required member.
    bad(&json!({"write_set": ["a"]}), "shape", Some("unit"));
    bad(&json!({"unit": "a"}), "shape", Some("write_set"));
}

#[test]
fn b3_the_members_the_page_shapes_and_the_bounds_are_accepted() {
    let s = S::new();
    s.change();
    let mut write_set = vec!["src/**"; 39];
    write_set.push("docs/d.md");
    let b = json!({"unit": "A-z-9", "write_set": write_set,
        "repository": "owner/name", "branch": "ap/unit-x", "item": "k1",
        "tier": "x".repeat(40), "merge": false,
        "checks": {"unchanged": vec!["zzz"; 40], "lines": [{"path": "nothing", "max": 0, "min": 0}],
            "adds": [{"path": "src/n.rs", "words": ["w".repeat(2000)]}],
            "holds": [{"path": "src/n.rs", "exact": "x".repeat(65_536)}]}});
    // A block at every bound is read; its two checks that cannot hold are the only lines.
    let lines = s.lines(&b);
    let kinds: Vec<_> = lines
        .iter()
        .map(|l| l.split(": ").nth(2).unwrap())
        .collect();
    assert_eq!(kinds, ["adds", "holds"]);
}

#[test]
fn c1_a_member_twice_in_a_block_is_refused() {
    let one = r#""unit":"T-1","write_set":["src/**"]"#;
    for body in [
        format!(r#"{{{one},"checks":{{"adds":[{{"path":"a","words":["x"]}}]}},"checks":{{}}}}"#),
        r#"{"unit":"T-1","write_set":["a"],"write_set":["src/**"]}"#.to_owned(),
        format!(r#"{{{one},"checks":{{"lines":[{{"path":"a","max":-1,"max":9}}]}}}}"#),
        format!(r#"{{{one},"checks":{{"lines":[{{"path":"a","max":1}}],"lines":[]}}}}"#),
        format!(r#"{{{one},"unit":"T-2"}}"#),
    ] {
        let (code, detail) = refused(&format!("```unit\n{body}\n```\n"));
        assert_eq!(code, "page_block_invalid", "{body}");
        assert_eq!(detail["reason"], "not_an_object", "{body}");
    }
    // A colon, a quote or a brace inside a string is no member.
    let s = S::new();
    let mut b = unit(
        &["src/**"],
        json!({"adds": [{"path": "a", "words": ["a\": b\\\", {"]}]}),
    );
    b["tier"] = json!("x:y");
    assert_eq!(failed(&s.lines(&b), "adds"), 1);
}

#[test]
fn c2_a_typechange_counts_only_its_own_lines() {
    let s = S::new();
    s.put("f.rs", b"one\n");
    s.commit("f");
    git(
        &s.repo(),
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    std::fs::remove_file(s.repo().join("f.rs")).unwrap();
    std::os::unix::fs::symlink("g.rs", s.repo().join("f.rs")).unwrap();
    s.commit("link");
    let w = ["f.rs"];
    // One removed line and one added line (the link's target); a header line is neither.
    let lines = |max: u64, min: u64| {
        s.lines(&unit(
            &w,
            json!({"lines": [{"path": "f.rs", "max": max, "min": min}]}),
        ))
    };
    assert_eq!(lines(2, 2), ["accept: ok"]);
    let words =
        json!({"adds": [{"path": "f.rs", "words": ["++ b/f.rs", "--- /dev/null", "f.rs"]}]});
    assert_eq!(failed(&s.lines(&unit(&w, words)), "adds"), 3);
    let words = json!({"adds": [{"path": "f.rs", "words": ["g.rs"]}]});
    assert_eq!(s.lines(&unit(&w, words)), ["accept: ok"]);
}

#[test]
fn c3_the_diff_starts_at_the_merge_base_of_origin_main_and_head() {
    let s = S::new();
    let base = git(&s.repo(), &["rev-parse", "HEAD"]);
    s.change();
    // `origin/main` moves on without the branch: its own change is no part of the branch's.
    git(&s.repo(), &["checkout", "-q", "--detach", &base]);
    s.put("main.txt", b"main\n");
    s.commit("main moves");
    let moved = git(&s.repo(), &["rev-parse", "HEAD"]);
    git(
        &s.repo(),
        &["update-ref", "refs/remotes/origin/main", &moved],
    );
    git(&s.repo(), &["checkout", "-q", "main"]);
    let checks = json!({"unchanged": ["main.txt"], "lines": [{"path": "main.txt", "max": 0}],
        "adds": [{"path": "src/n.rs", "words": ["fn n()"]}]});
    assert_eq!(
        s.lines(&unit(&["src/**", "docs/d.md"], checks)),
        ["accept: ok"]
    );
}

#[test]
fn c4_the_block_ends_at_the_next_line_of_three_backticks() {
    let s = S::new();
    s.change();
    let block = unit(&["src/**", "docs/d.md"], json!({}));
    let page = s.page(
        "p.md",
        &format!("# T\n```unit\n{block}\n```\n\nMore:\n```text\nx\n```\n"),
    );
    assert_eq!(s.accept(&[&page]).1, ["accept: ok"]);
}

#[test]
fn c5_a_renamed_file_is_its_old_path_removed_and_its_new_path_added() {
    let s = S::new();
    git(&s.repo(), &["mv", "src/a.rs", "src/r.rs"]);
    s.commit("rename");
    let w = ["src/**"];
    let one = |checks: Value| s.lines(&unit(&w, checks));
    assert_eq!(
        failed(&one(json!({"unchanged": ["src/a.rs"]})), "unchanged"),
        1
    );
    assert_eq!(
        failed(&one(json!({"unchanged": ["src/r.rs"]})), "unchanged"),
        1
    );
    // Three lines removed, three added: a rename is not one change of zero lines.
    assert_eq!(
        one(json!({"lines": [{"path": "src/**", "max": 6, "min": 6}]})),
        ["accept: ok"]
    );
}
