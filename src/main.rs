mod application_icon;
mod cli;
mod diff;
mod diff_viewer;
mod icons;

use std::process::ExitCode;

fn main() -> ExitCode {
    let options = cli::parse();
    diff_viewer::run(options);
    ExitCode::SUCCESS
}
