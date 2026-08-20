use std::path::PathBuf;

use bpaf::{Parser, construct, long, positional, short};

const VERSION_TEXT: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Options {
    pub(super) repo: Option<PathBuf>,
    pub(super) command: Command,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Command {
    Diff(DiffOptions),
    Show(ShowOptions),
    StashShow(StashShowOptions),
    Patch(PatchOptions),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CliAction {
    Run(Options),
    Version,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DiffOptions {
    pub(super) watch: bool,
    pub(super) staged: bool,
    pub(super) exclude_untracked: bool,
    pub(super) targets: Vec<String>,
    pub(super) pathspecs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ShowOptions {
    pub(super) target: Option<String>,
    pub(super) pathspecs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StashShowOptions {
    pub(super) reference: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PatchOptions {
    pub(super) file: Option<PathBuf>,
}

fn diff_parser() -> impl Parser<Command> {
    let watch = long("watch")
        .help("Reload the diff when watched files change")
        .switch();
    let staged = long("staged").help("Show staged changes").switch();
    let exclude_untracked = long("exclude-untracked")
        .help("Do not include untracked files")
        .switch();
    let targets = positional::<String>("TARGET")
        .help("Revision, range, or two files to compare")
        .non_strict()
        .many();
    let pathspecs = positional::<String>("PATHSPEC")
        .help("Limit the diff to paths after --")
        .strict()
        .many();

    construct!(DiffOptions {
        watch,
        staged,
        exclude_untracked,
        targets,
        pathspecs,
    })
    .map(Command::Diff)
    .to_options()
    .descr("Show working-tree, staged, revision, or file changes")
    .command("diff")
}

fn show_parser() -> impl Parser<Command> {
    let target = positional::<String>("TARGET")
        .help("Commit to show (defaults to HEAD)")
        .non_strict()
        .optional();
    let pathspecs = positional::<String>("PATHSPEC")
        .help("Limit the commit diff to paths after --")
        .strict()
        .many();

    construct!(ShowOptions { target, pathspecs })
        .map(Command::Show)
        .to_options()
        .descr("Show the changes introduced by a commit")
        .command("show")
}

fn stash_parser() -> impl Parser<Command> {
    let reference = positional::<String>("REF")
        .help("Stash reference (defaults to stash@{0})")
        .optional();
    let show = construct!(StashShowOptions { reference })
        .map(Command::StashShow)
        .to_options()
        .descr("Show the changes stored in a stash")
        .command("show");

    show.to_options()
        .descr("Inspect Git stashes")
        .command("stash")
}

fn patch_parser() -> impl Parser<Command> {
    let file = positional::<PathBuf>("FILE")
        .help("Unified diff file; omit or use - to read stdin")
        .optional();

    construct!(PatchOptions { file })
        .map(Command::Patch)
        .to_options()
        .descr("Show a unified diff file or stdin")
        .command("patch")
}

fn command_parser() -> impl Parser<Command> {
    construct!([diff_parser(), show_parser(), stash_parser(), patch_parser(),])
}

fn options_parser() -> impl Parser<Options> {
    let repo = long("repo")
        .help("Select the repository for Git-backed commands")
        .argument::<PathBuf>("PATH")
        .optional();
    let command = command_parser();

    construct!(Options { repo, command })
}

#[cfg(test)]
pub(super) fn options() -> bpaf::OptionParser<Options> {
    options_parser()
        .to_options()
        .descr("GPU-accelerated Git diff viewer")
        .fallback_to_usage()
}

pub(super) fn parse() -> Options {
    let version = short('V')
        .long("version")
        .help("Print version information")
        .req_flag(CliAction::Version);
    let run = options_parser().map(CliAction::Run);
    let parser = construct!([version, run])
        .to_options()
        .descr("GPU-accelerated Git diff viewer")
        .fallback_to_usage();

    match parser.run() {
        CliAction::Run(options) => options,
        CliAction::Version => {
            println!("{VERSION_TEXT}");
            std::process::exit(0);
        }
    }
}
