#[cfg(test)]
use std::ffi::OsStr;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Options {
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

#[derive(Clone, Debug, Eq, PartialEq, usage::Args)]
pub(super) struct DiffOptions {
    /// Reload the diff when watched files change.
    #[usage(long)]
    pub(super) watch: bool,

    /// Show staged changes.
    #[usage(long)]
    pub(super) staged: bool,

    /// Do not include untracked files.
    #[usage(long)]
    pub(super) exclude_untracked: bool,

    /// Revision, range, or two files to compare.
    #[usage(name = "TARGET")]
    pub(super) targets: Vec<String>,

    /// Limit the diff to paths after --.
    #[usage(name = "PATHSPEC", double_dash = "required")]
    pub(super) pathspecs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, usage::Args)]
pub(super) struct ShowOptions {
    /// Commit to show (defaults to HEAD).
    #[usage(name = "TARGET")]
    pub(super) target: Option<String>,

    /// Limit the commit diff to paths after --.
    #[usage(name = "PATHSPEC", double_dash = "required")]
    pub(super) pathspecs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, usage::Args)]
pub(super) struct StashShowOptions {
    /// Stash reference (defaults to stash@{0}).
    #[usage(name = "REF")]
    pub(super) reference: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, usage::Args)]
pub(super) struct PatchOptions {
    /// Unified diff file; omit or use - to read stdin.
    #[usage(name = "FILE")]
    pub(super) file: Option<PathBuf>,
}

/// GPU-accelerated Git diff viewer.
#[derive(Debug, usage::Cli)]
#[usage(bin = "sabun", version, unknown_flags = "error")]
struct CliOptions {
    /// Select the repository for Git-backed commands.
    #[usage(long, global, value_name = "PATH")]
    repo: Option<PathBuf>,

    #[usage(subcommand)]
    command: CliCommand,
}

#[derive(Debug, usage::Subcommands)]
enum CliCommand {
    /// Show working-tree, staged, revision, or file changes.
    Diff(DiffOptions),

    /// Show the changes introduced by a commit.
    Show(ShowOptions),

    /// Inspect Git stashes.
    Stash(StashOptions),

    /// Show a unified diff file or stdin.
    Patch(PatchOptions),
}

#[derive(Debug, usage::Args)]
struct StashOptions {
    #[usage(subcommand)]
    command: StashCommand,
}

#[derive(Debug, usage::Subcommands)]
enum StashCommand {
    /// Show the changes stored in a stash.
    Show(StashShowOptions),
}

impl From<CliOptions> for Options {
    fn from(options: CliOptions) -> Self {
        Self {
            repo: options.repo,
            command: match options.command {
                CliCommand::Diff(options) => Command::Diff(options),
                CliCommand::Show(options) => Command::Show(options),
                CliCommand::Stash(StashOptions {
                    command: StashCommand::Show(options),
                }) => Command::StashShow(options),
                CliCommand::Patch(options) => Command::Patch(options),
            },
        }
    }
}

#[cfg(test)]
pub(super) fn parse_from(args: &[&str]) -> Result<Options, String> {
    let args = args.iter().map(OsStr::new).collect::<Vec<_>>();
    CliOptions::parse_from(&args)
        .map(Options::from)
        .map_err(|error| format!("{error:?}"))
}

pub(super) fn parse() -> Options {
    CliOptions::parse().into()
}

#[cfg(test)]
mod tests {
    use super::CliOptions;

    #[test]
    fn generated_usage_spec_is_coherent() {
        assert!(!CliOptions::to_kdl().is_empty());
    }
}
