use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn run() -> ap_kpi_producer::Result<u8> {
    use ap_kpi_producer::{
        Error, capture, filesystem, journal, now, process::NativeRunner, strict,
    };
    use std::time::{Duration, Instant};
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        use std::io::Write;
        std::io::stdout().lock().write_all(b"Usage: ap-kpi capture --request /absolute/owner-request.json\nOwner-side capture and append; requires explicit capture authority.\n").map_err(|_|Error("stdout"))?;
        return Ok(0);
    }
    if args.len() != 3 || args[0] != "capture" || args[1] != "--request" {
        return Err(Error("usage"));
    }
    let path = args[2].to_str().ok_or(Error("invalid_path"))?;
    let bytes = filesystem::read(path, 65536)?;
    let exe =
        std::fs::File::open(std::env::current_exe().map_err(|_| Error("binary_pin_unavailable"))?)
            .map_err(|_| Error("binary_pin_unavailable"))?;
    let binary = filesystem::bounded(exe, 64 * 1024 * 1024)?;
    let p = capture::prepare(&bytes, &ap_kpi_producer::digest(&binary))?;
    let path = p.request.journal_path.clone();
    let mut runner = NativeRunner {
        per_process: Duration::from_secs(p.request.limits.process_timeout_seconds.into()),
        deadline: Instant::now()
            + Duration::from_secs(p.request.limits.capture_timeout_seconds.into()),
        output_limit: p.request.limits.process_output_bytes as usize,
    };
    let (snapshot, evidence) = capture::collect(p, &mut runner, &mut now, &mut capture::disk_free)?;
    evidence.verify()?;
    let receipt = journal::append(&path, &snapshot)?;
    // Evidence can be replaced while prior journal rows are validated. Never
    // acknowledge an append whose advertised artifacts no longer read back.
    evidence.verify().map_err(|_| Error("append_unknown"))?;
    use std::io::Write;
    let mut bytes = strict::bytes(&receipt).map_err(|_| Error("append_unknown"))?;
    bytes.push(b'\n');
    std::io::stdout()
        .lock()
        .write_all(&bytes)
        .map_err(|_| Error("append_unknown"))?;
    Ok(if snapshot.capture_status == "complete" {
        0
    } else {
        3
    })
}
fn main() -> ExitCode {
    #[cfg(target_os = "linux")]
    {
        match run() {
            Ok(code) => ExitCode::from(code),
            Err(e) => {
                use std::io::Write;
                let _ = writeln!(std::io::stderr().lock(), "{{\"error\":\"{}\"}}", e.0);
                ExitCode::from(e.exit_code())
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("{{\"error\":\"unsupported_host\"}}");
        ExitCode::from(2)
    }
}
