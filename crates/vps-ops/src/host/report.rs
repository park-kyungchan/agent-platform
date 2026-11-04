use crate::{
    Result,
    runner::{Runner, checked},
    util,
};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Debug)]
pub struct ReportInput {
    pub old: String,
    pub new: String,
    pub commits: String,
    pub changed: String,
    pub shortstat: String,
    pub count: u64,
    pub time: String,
}
pub fn render(v: &ReportInput) -> String {
    let rules: Vec<(String, Vec<String>)> =
        serde_json::from_str(include_str!("report-rules.json")).expect("report rules");
    let commits: Vec<_> = v
        .commits
        .lines()
        .map(|l| l.split_once('\t').unwrap_or((l, "")))
        .collect();
    let changed: Vec<_> = v.changed.lines().filter(|l| !l.is_empty()).collect();
    let mut categories = vec![Vec::new(); rules.len()];
    let mut areas: Vec<(&str, usize)> = vec![];
    let mut indices = BTreeMap::new();
    let mut attention = vec![];
    for (sha, subject) in &commits {
        let lowered = subject.to_lowercase();
        if let Some(i) = rules
            .iter()
            .position(|(_, words)| words.iter().any(|w| lowered.contains(w)))
        {
            categories[i].push(format!(
                "`{}` {subject}",
                sha.chars().take(8).collect::<String>()
            ))
        }
        if [
            "breaking",
            "migration",
            "schema",
            "remove",
            "deprecat",
            "rename",
            "default",
        ]
        .iter()
        .any(|w| lowered.contains(w))
        {
            attention.push(format!(
                "`{}` {subject}",
                sha.chars().take(8).collect::<String>()
            ))
        }
    }
    for path in &changed {
        let name = path.split('/').next().unwrap_or(path);
        let i = *indices.entry(name).or_insert_with(|| {
            let i = areas.len();
            areas.push((name, 0));
            i
        });
        areas[i].1 += 1
    }
    areas.sort_by_key(|b| std::cmp::Reverse(b.1));
    let gates = [
        "Dockerfile",
        "pyproject.toml",
        "uv.lock",
        "package.json",
        "package-lock.json",
        "docker-compose.yml",
    ];
    let hits: Vec<_> = changed
        .iter()
        .filter(|p| gates.contains(&p.rsplit('/').next().unwrap_or(p)))
        .collect();
    let mut lines = vec![
        "# Hermes main 업데이트 보고서".to_owned(),
        String::new(),
        format!("- 생성 시각(UTC): {}", v.time),
        format!("- 현재 커밋: `{}`", v.old),
        format!("- 후보 커밋: `{}`", v.new),
        format!("- 차이: **{} commits**", v.count),
        format!(
            "- 파일 변경: {}",
            if v.shortstat.is_empty() {
                "No file changes"
            } else {
                &v.shortstat
            }
        ),
        String::new(),
        "## 변경 영역".into(),
        String::new(),
    ];
    if areas.is_empty() {
        lines.push("- 변경 파일 없음".into())
    } else {
        for (name, n) in areas.iter().take(20) {
            lines.push(format!("- `{name}`: {n} files"))
        }
    }
    lines.extend([
        String::new(),
        "## 빌드·마이그레이션 주의 파일".into(),
        String::new(),
    ]);
    if hits.is_empty() {
        lines.push("- 주요 빌드/lock 파일 변경 없음".into())
    } else {
        lines.extend(hits.iter().map(|p| format!("- `{p}`")))
    }
    lines.extend([
        String::new(),
        "## 주의가 필요한 커밋 제목".into(),
        String::new(),
    ]);
    if attention.is_empty() {
        lines.push("- 제목 기준의 명시적 breaking/migration 신호 없음".into())
    } else {
        lines.extend(attention.iter().take(40).map(|s| format!("- {s}")))
    }
    for ((name, _), items) in rules.iter().zip(categories) {
        if !items.is_empty() {
            lines.extend([
                String::new(),
                format!("## {name} ({})", items.len()),
                String::new(),
            ]);
            lines.extend(items.iter().take(25).map(|s| format!("- {s}")));
            if items.len() > 25 {
                lines.push(format!("- …외 {}개", items.len() - 25))
            }
        }
    }
    lines.extend([
        String::new(),
        "## 최근 커밋 전체 보기 (최대 60개)".into(),
        String::new(),
    ]);
    for (sha, subject) in commits.iter().take(60) {
        lines.push(format!(
            "- `{}` {subject}",
            sha.chars().take(8).collect::<String>()
        ))
    }
    if commits.len() > 60 {
        lines.push(format!("- …외 {}개", commits.len() - 60))
    }
    lines.extend([String::new(),"> 이 보고서는 커밋 제목과 변경 파일을 자동 분류한 것입니다. 실제 배포 전 이미지 스모크 테스트와 전체 백업 검증을 별도로 수행합니다.".into(),String::new()]);
    lines.join("\n")
}
pub fn run(r: &mut dyn Runner, args: &[String]) -> Result<()> {
    if args.len() != 4 {
        return Err("usage: hermes-main-report REPO OLD_SHA NEW_SHA OUTPUT".into());
    }
    let (repo, old, new, output) = (&args[0], &args[1], &args[2], &args[3]);
    let mut git = |a: Vec<String>| -> Result<String> {
        let mut all = vec!["-C".into(), repo.clone()];
        all.extend(a);
        checked(r, "git", all)
    };
    let range = format!("{old}..{new}");
    let v = ReportInput {
        old: old.clone(),
        new: new.clone(),
        commits: git(vec![
            "log".into(),
            "--no-merges".into(),
            "--format=%H%x09%s".into(),
            range.clone(),
        ])?,
        changed: git(vec![
            "diff".into(),
            "--name-only".into(),
            old.clone(),
            new.clone(),
        ])?,
        shortstat: git(vec![
            "diff".into(),
            "--shortstat".into(),
            old.clone(),
            new.clone(),
        ])?,
        count: git(vec!["rev-list".into(), "--count".into(), range])?.parse()?,
        time: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false),
    };
    std::fs::write(Path::new(output), render(&v))?;
    Ok(())
}
pub fn source_identity() -> String {
    util::hash(include_bytes!("report-rules.json"))
}
