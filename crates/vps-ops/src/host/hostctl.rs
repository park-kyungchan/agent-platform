use crate::{
    Result,
    runner::{CommandSpec, Runner},
};
pub struct HostOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}
fn call(
    r: &mut dyn Runner,
    program: &str,
    args: &[&str],
    stderr: &mut String,
) -> Result<(i32, String)> {
    let v = r.run(CommandSpec::new(
        program,
        args.iter().map(|s| s.to_string()).collect(),
    ))?;
    stderr.push_str(&v.stderr);
    Ok((v.exit_code, v.stdout))
}
/// Host diagnostics for the retained SSH and Docker services.
pub fn dispatch(r: &mut dyn Runner, cmd: &str) -> Result<HostOutput> {
    let mut out = String::new();
    let mut err = String::new();
    let mut code = 0;
    macro_rules! run {
        ($p:expr,$a:expr) => {{
            let (c, s) = call(r, $p, $a, &mut err)?;
            out.push_str(&s);
            if c != 0 {
                return Ok(HostOutput {
                    code: c,
                    stdout: out,
                    stderr: err,
                });
            }
        }};
    }
    match cmd {
        "inventory" => {
            out.push_str("=== HOST INVENTORY ===\nhostname=");
            run!("hostname", &[]);
            out.push_str("kernel=");
            run!("uname", &["-sr"]);
            out.push_str("os=");
            let (c, s) = call(r, "cat", &["/etc/os-release"], &mut err)?;
            if c != 0 {
                return Ok(HostOutput {
                    code: c,
                    stdout: out,
                    stderr: err,
                });
            }
            let get = |k: &str| {
                s.lines()
                    .find_map(|l| {
                        l.strip_prefix(&format!("{k}="))
                            .map(|v| v.trim_matches('"'))
                    })
                    .unwrap_or("")
            };
            out.push_str(&format!("{} {}\n", get("ID"), get("VERSION_ID")));
            out.push_str("uptime=");
            run!("uptime", &["-p"]);
            out.push_str("cpu=");
            run!("nproc", &[]);
            out.push_str("memory=");
            let (c, s) = call(r, "free", &["-h"], &mut err)?;
            if c != 0 {
                return Ok(HostOutput {
                    code: c,
                    stdout: out,
                    stderr: err,
                });
            }
            if let Some(l) = s.lines().find(|l| l.starts_with("Mem:")) {
                let f: Vec<_> = l.split_whitespace().collect();
                if f.len() >= 7 {
                    out.push_str(&format!("{} total, {} available\n", f[1], f[6]))
                }
            }
            out.push_str("disk=");
            let (c, s) = call(r, "df", &["-hT", "/"], &mut err)?;
            if c != 0 {
                return Ok(HostOutput {
                    code: c,
                    stdout: out,
                    stderr: err,
                });
            }
            if let Some(l) = s.lines().nth(1) {
                let f: Vec<_> = l.split_whitespace().collect();
                if f.len() >= 5 {
                    out.push_str(&format!(
                        "{} total, {} used, {} available\n",
                        f[2], f[4], f[3]
                    ))
                }
            }
            out.push_str("--- SERVICES ---\n");
            for name in ["ssh.socket", "docker"] {
                out.push_str(&format!("{name}="));
                let (_, s) = call(r, "systemctl", &["is-active", name], &mut String::new())?;
                out.push_str(&s)
            }
            out.push_str("--- DOCKER ---\n");
            run!(
                "/usr/bin/docker",
                &[
                    "ps",
                    "--format",
                    "table {{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}"
                ]
            );
        }
        "resources" => {
            out.push_str("=== RESOURCES ===\n");
            run!("uptime", &[]);
            run!("free", &["-h"]);
            let (_, s) = call(r, "df", &["-hT", "/", "/docker"], &mut String::new())?;
            out.push_str(&s)
        }
        "docker.status" => {
            out.push_str("=== DOCKER STATUS ===\n");
            run!(
                "/usr/bin/docker",
                &[
                    "ps",
                    "--format",
                    "table {{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}"
                ]
            );
            run!(
                "/usr/bin/docker",
                &[
                    "stats",
                    "--no-stream",
                    "--format",
                    "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}"
                ]
            );
        }
        "ssh.status" => {
            out.push_str("=== SSH STATUS ===\n");
            let (c, s) = call(r, "sshd", &["-T"], &mut err)?;
            let mut lines: Vec<_> = s
                .lines()
                .filter(|l| {
                    [
                        "addressfamily",
                        "listenaddress",
                        "port",
                        "permitrootlogin",
                        "passwordauthentication",
                    ]
                    .iter()
                    .any(|k| l.starts_with(k))
                })
                .collect();
            lines.sort();
            let empty = lines.is_empty();
            for l in lines {
                out.push_str(l);
                out.push('\n')
            }
            // grep and sort finish writing before Bash observes the pipefail status.
            if c != 0 || empty {
                return Ok(HostOutput {
                    code: if empty { 1 } else { c },
                    stdout: out,
                    stderr: err,
                });
            }
            let (_, s) = call(r, "ss", &["-lntp"], &mut err)?;
            for l in s.lines().filter(|l| l.contains(":22")) {
                out.push_str(l);
                out.push('\n')
            }
        }
        _ => {
            code = 64;
            return Ok(HostOutput {
                code,
                stdout: out,
                stderr: "DENIED: unsupported hostctl command\n".into(),
            });
        }
    }
    Ok(HostOutput {
        code,
        stdout: out,
        stderr: err,
    })
}
