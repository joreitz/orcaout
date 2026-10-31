use std::process::ExitCode;

use orcaout::cli::{self, CliAction};

fn main() -> ExitCode {
    let action = match cli::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    match action {
        CliAction::Help => {
            print!("{}", cli::USAGE);
            ExitCode::SUCCESS
        }
        CliAction::Version => {
            println!("orcaout {}", cli::VERSION);
            ExitCode::SUCCESS
        }
        CliAction::Run(args) => match orcaout::app::run(&args) {
            Ok(code) => ExitCode::from(code.clamp(0, 255) as u8),
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(1)
            }
        },
    }
}
