use ap_mem_adapter::{cli, protocol};
use std::{io, process::ExitCode};

fn main() -> ExitCode {
    match cli::parse(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(cli::Command::Mcp) => {
            if protocol::serve(io::stdin().lock(), io::stdout().lock()).is_err() {
                eprintln!("stdio_failure");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(cli::Command::Publish { .. }) => {
            eprintln!("publication_unavailable");
            ExitCode::from(2)
        }
        Err(usage) => {
            eprintln!("{usage}");
            ExitCode::from(2)
        }
    }
}
