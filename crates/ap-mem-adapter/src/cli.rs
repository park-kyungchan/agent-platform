#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Mcp,
    Publish { task: String },
}

/// Matches ont's attribution grammar; task is attribution, never authority.
pub fn parse(args: &[String]) -> Result<Command, &'static str> {
    match args {
        [mcp] if mcp == "mcp" => Ok(Command::Mcp),
        [publish, flag, task]
            if publish == "publish"
                && flag == "--task"
                && !task.is_empty()
                && task.len() <= 64
                && task.bytes().all(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-')
                }) =>
        {
            Ok(Command::Publish { task: task.clone() })
        }
        _ => Err("usage: ap-mem-adapter mcp | publish --task <task-name>"),
    }
}
