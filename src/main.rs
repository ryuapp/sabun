mod application_icon;
mod cli;
mod diff;
mod diff_viewer;
mod icons;

use std::process::ExitCode;

fn main() -> ExitCode {
    let launch = match cli::load() {
        Ok(launch) => launch,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    diff_viewer::run(launch);
    ExitCode::SUCCESS
}
