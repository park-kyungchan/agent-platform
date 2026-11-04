use std::collections::HashSet;

use crate::{Artifact, Entry, ErrorCode, RenderError, RenderPlan, RenderRequest, Selector};

fn error(code: ErrorCode, field: &str) -> RenderError {
    RenderError {
        code,
        field: field.into(),
    }
}

fn path_chars(path: &str) -> bool {
    path.bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'))
}

fn absolute_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() > 1
        && path_chars(path)
        && path[1..]
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn link_target_syntax(target: &str) -> bool {
    if target.is_empty() || target == "/" || !path_chars(target) {
        return false;
    }
    let components = target.strip_prefix('/').unwrap_or(target);
    components.split('/').all(|part| !part.is_empty())
}

fn link_matches(target: &str, config_dir: &str, canonical: &str) -> Option<bool> {
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        config_dir[1..].split('/').collect()
    };
    for part in target.strip_prefix('/').unwrap_or(target).split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("/{}", parts.join("/")) == canonical)
}

fn claude_file(text: &str, canonical: &str) -> Result<String, RenderError> {
    let field = "claude.instructions";
    let bytes = text.as_bytes();
    let mut has_lf = false;
    let mut has_crlf = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0 => return Err(error(ErrorCode::UnsupportedText, field)),
            b'\r' => {
                if bytes.get(i + 1) != Some(&b'\n') {
                    return Err(error(ErrorCode::UnsupportedText, field));
                }
                has_crlf = true;
                i += 1;
            }
            b'\n' => has_lf = true,
            _ => {}
        }
        i += 1;
    }
    if has_lf && has_crlf {
        return Err(error(ErrorCode::UnsupportedText, field));
    }
    let eol = if has_crlf { "\r\n" } else { "\n" };
    let import = format!("@{canonical}");
    let mut imports = 0;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            return Err(error(ErrorCode::InstructionConflict, field));
        }
        if line == import {
            imports += 1;
            if imports > 1 {
                return Err(error(ErrorCode::InstructionConflict, field));
            }
        } else if line.contains(canonical) {
            return Err(error(ErrorCode::InstructionConflict, field));
        }
    }
    if imports == 1 {
        return Ok(text.to_owned());
    }
    let mut result = text.to_owned();
    if !text.is_empty() && !text.ends_with('\n') {
        result.push_str(eol);
    }
    result.push_str(&import);
    result.push_str(eol);
    Ok(result)
}

fn instruction_link(
    entry: &Entry,
    config_dir: &str,
    canonical: &str,
    field: &str,
    target_field: &str,
) -> Result<Entry, RenderError> {
    match entry {
        Entry::Missing => Ok(Entry::Symlink {
            target: canonical.into(),
        }),
        Entry::File { .. } => Err(error(ErrorCode::InstructionConflict, field)),
        Entry::Unsupported { .. } => Err(error(ErrorCode::UnsupportedTarget, field)),
        Entry::Symlink { target } => {
            if !link_target_syntax(target) {
                return Err(error(ErrorCode::InvalidPath, target_field));
            }
            match link_matches(target, config_dir, canonical) {
                None => Err(error(ErrorCode::UnsupportedTarget, field)),
                Some(false) => Err(error(ErrorCode::InstructionConflict, field)),
                Some(true) => Ok(entry.clone()),
            }
        }
    }
}

/// Propose explicit owned native instruction entries without external effects.
pub fn render(request: &RenderRequest) -> Result<RenderPlan, RenderError> {
    if request.schema_version != 1 {
        return Err(error(ErrorCode::UnsupportedSchema, "schema_version"));
    }
    for (path, field) in [
        (&request.canonical_path, "canonical_path"),
        (&request.codex.config_dir, "codex.config_dir"),
        (&request.claude.config_dir, "claude.config_dir"),
    ] {
        if !absolute_path(path) {
            return Err(error(ErrorCode::InvalidPath, field));
        }
    }
    if let Some(agy) = &request.agy
        && !absolute_path(&agy.config_dir)
    {
        return Err(error(ErrorCode::InvalidPath, "agy.config_dir"));
    }
    for entry in &request.preserved {
        if !absolute_path(&entry.path) {
            return Err(error(ErrorCode::InvalidPath, "preserved.path"));
        }
    }

    let codex_path = format!("{}/AGENTS.md", request.codex.config_dir);
    let override_path = format!("{}/AGENTS.override.md", request.codex.config_dir);
    let claude_path = format!("{}/CLAUDE.md", request.claude.config_dir);
    let agy_paths = request.agy.as_ref().map(|agy| {
        [
            format!("{}/AGENTS.md", agy.config_dir),
            format!("{}/GEMINI.md", agy.config_dir),
        ]
    });
    let mut owned_paths = vec![&codex_path, &override_path, &claude_path];
    if let Some(paths) = &agy_paths {
        owned_paths.extend(paths);
    }
    if request.codex.config_dir == request.claude.config_dir
        || request.agy.as_ref().is_some_and(|agy| {
            agy.config_dir == request.codex.config_dir
                || agy.config_dir == request.claude.config_dir
        })
        || owned_paths.contains(&&request.canonical_path)
    {
        return Err(error(ErrorCode::PathCollision, "profiles"));
    }
    let mut seen = HashSet::new();
    for entry in &request.preserved {
        if !seen.insert(&entry.path) {
            return Err(error(ErrorCode::DuplicatePreservedPath, "preserved.path"));
        }
    }
    for entry in &request.preserved {
        if entry.path == request.canonical_path || owned_paths.contains(&&entry.path) {
            return Err(error(ErrorCode::PathCollision, "preserved.path"));
        }
    }
    if request.codex.instruction_override != Entry::Missing {
        return Err(error(
            ErrorCode::InstructionConflict,
            "codex.instruction_override",
        ));
    }
    let codex_after = instruction_link(
        &request.codex.instructions,
        &request.codex.config_dir,
        &request.canonical_path,
        "codex.instructions",
        "codex.instructions.target",
    )?;
    let claude_after = match &request.claude.instructions {
        Entry::Missing => Entry::File {
            text: format!("@{}\n", request.canonical_path),
        },
        Entry::File { text } => Entry::File {
            text: claude_file(text, &request.canonical_path)?,
        },
        Entry::Symlink { .. } | Entry::Unsupported { .. } => {
            return Err(error(ErrorCode::UnsupportedTarget, "claude.instructions"));
        }
    };
    let mut artifacts = vec![
        Artifact {
            path: codex_path,
            selector: Selector::CodexInstructionLink,
            before: request.codex.instructions.clone(),
            changed: request.codex.instructions != codex_after,
            after: codex_after,
        },
        Artifact {
            path: claude_path,
            selector: Selector::ClaudeCanonicalImport,
            before: request.claude.instructions.clone(),
            changed: request.claude.instructions != claude_after,
            after: claude_after,
        },
    ];
    if let Some(agy) = &request.agy {
        for (name, selector, before, field, target_field) in [
            (
                "AGENTS.md",
                Selector::AgyInstructionLink,
                &agy.instructions,
                "agy.instructions",
                "agy.instructions.target",
            ),
            (
                "GEMINI.md",
                Selector::AgyGeminiAlias,
                &agy.gemini_alias,
                "agy.gemini_alias",
                "agy.gemini_alias.target",
            ),
        ] {
            let after = instruction_link(
                before,
                &agy.config_dir,
                &request.canonical_path,
                field,
                target_field,
            )?;
            artifacts.push(Artifact {
                path: format!("{}/{name}", agy.config_dir),
                selector,
                before: before.clone(),
                changed: *before != after,
                after,
            });
        }
    }
    artifacts.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let mut preserved = request.preserved.clone();
    preserved.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    Ok(RenderPlan {
        schema_version: 1,
        artifacts,
        preserved,
    })
}
