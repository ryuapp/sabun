mod arguments;
mod repository;
#[cfg(test)]
mod tests;

use std::{
    fs,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use similar::TextDiff;

use crate::diff::{DiffSet, parse_unified_diff};
pub(super) use arguments::Options;
use arguments::{Command, DiffOptions, PatchOptions, ShowOptions, StashShowOptions};
use repository::{
    DiffRequest, load_commit_page, load_diff, load_show, load_source_catalog, load_stash,
    load_worktrees, watch_paths, worktree_root,
};

#[derive(Clone)]
pub(super) struct Input {
    pub(crate) diff: DiffSet,
    pub(crate) path_root: Option<PathBuf>,
    pub(crate) source_name: String,
    pub(crate) comparison_label: String,
    pub(crate) target_label: String,
    pub(crate) empty_title: String,
    pub(crate) empty_detail: String,
}

pub(super) struct Launch {
    pub(super) input: Input,
    pub(super) watch: Option<WatchRequest>,
    pub(super) source_switcher: Option<GitDiffSourceSwitcher>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GitDiffSource {
    Changes,
    Staged(Option<String>),
    Comparison(String),
    Commit(Option<String>),
    Stash(Option<String>),
}

impl GitDiffSource {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Changes => "Changes".into(),
            Self::Staged(_) => "Staged changes".into(),
            Self::Comparison(target) => format!("Compare {target}"),
            Self::Commit(Some(target)) => format!("Commit {}", abbreviated(target)),
            Self::Commit(None) => "Latest commit".into(),
            Self::Stash(Some(reference)) => reference.clone(),
            Self::Stash(None) => "Latest stash".into(),
        }
    }
}

fn abbreviated(value: &str) -> &str {
    value.get(..7).unwrap_or(value)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitCommitSource {
    pub(crate) oid: String,
    pub(crate) short_oid: String,
    pub(crate) summary: String,
    pub(crate) author: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitStashSource {
    pub(crate) reference: String,
    pub(crate) short_oid: String,
    pub(crate) summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitCommitPage {
    pub(crate) commits: Vec<GitCommitSource>,
    pub(crate) has_more: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitSourceCatalog {
    pub(crate) commits: Vec<GitCommitSource>,
    pub(crate) has_more_commits: bool,
    pub(crate) stashes: Vec<GitStashSource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitWorktree {
    pub(crate) name: String,
    pub(crate) branch: String,
    pub(crate) path: PathBuf,
}

#[derive(Clone)]
pub(crate) struct GitDiffSourceSwitcher {
    directory: Arc<Mutex<PathBuf>>,
    source: Arc<Mutex<GitDiffSource>>,
    worktrees: Arc<Vec<GitWorktree>>,
    include_untracked: bool,
    pathspecs: Vec<String>,
}

impl GitDiffSourceSwitcher {
    fn directory(&self) -> PathBuf {
        self.directory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn worktrees(&self) -> &[GitWorktree] {
        &self.worktrees
    }

    pub(crate) fn current_worktree(&self) -> Option<GitWorktree> {
        let directory = self.directory();
        self.worktrees
            .iter()
            .find(|worktree| worktree.path == directory)
            .cloned()
    }

    pub(crate) fn source(&self) -> GitDiffSource {
        self.source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn switch_to(&self, source: GitDiffSource) -> Result<Input, String> {
        let input = self.load_source(&self.directory(), &source)?;
        *self
            .source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = source;
        Ok(input)
    }

    pub(crate) fn switch_worktree(&self, path: PathBuf) -> Result<Input, String> {
        if !self.worktrees.iter().any(|worktree| worktree.path == path) {
            return Err(format!("Unknown Git worktree: {}", path.display()));
        }
        let source = GitDiffSource::Changes;
        let input = self.load_source(&path, &source)?;
        *self
            .directory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = path;
        *self
            .source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = source;
        Ok(input)
    }

    pub(crate) fn catalog(&self) -> Result<GitSourceCatalog, String> {
        load_source_catalog(&self.directory())
    }

    pub(crate) fn commit_page(&self, offset: usize) -> Result<GitCommitPage, String> {
        load_commit_page(&self.directory(), offset)
    }

    fn reload(&self) -> Result<Input, String> {
        self.load_source(&self.directory(), &self.source())
    }

    fn load_source(&self, directory: &Path, source: &GitDiffSource) -> Result<Input, String> {
        match source {
            GitDiffSource::Changes => load_diff(
                directory,
                &DiffRequest {
                    target: None,
                    staged: false,
                    include_untracked: self.include_untracked,
                    pathspecs: &self.pathspecs,
                },
            ),
            GitDiffSource::Staged(target) => load_diff(
                directory,
                &DiffRequest {
                    target: target.as_deref(),
                    staged: true,
                    include_untracked: false,
                    pathspecs: &self.pathspecs,
                },
            ),
            GitDiffSource::Comparison(target) => load_diff(
                directory,
                &DiffRequest {
                    target: Some(target),
                    staged: false,
                    include_untracked: self.include_untracked,
                    pathspecs: &self.pathspecs,
                },
            ),
            GitDiffSource::Commit(target) => {
                load_show(directory, target.as_deref(), &self.pathspecs)
            }
            GitDiffSource::Stash(reference) => load_stash(directory, reference.as_deref()),
        }
    }
}

#[derive(Clone)]
enum ReloadRequest {
    Options {
        options: Options,
        directory: PathBuf,
    },
    Git(GitDiffSourceSwitcher),
}

#[derive(Clone)]
pub(super) struct WatchRequest {
    reload: ReloadRequest,
    paths: Vec<PathBuf>,
}

impl WatchRequest {
    pub(super) fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub(super) fn reload(&self) -> Result<Input, String> {
        match &self.reload {
            ReloadRequest::Options { options, directory } => {
                execute_options(options.clone(), directory, &mut Cursor::new(Vec::new()))
            }
            ReloadRequest::Git(source_switcher) => source_switcher.reload(),
        }
    }
}

pub(super) fn parse() -> Options {
    arguments::parse()
}

pub(super) fn load(options: Options) -> Result<Launch, String> {
    let directory = std::env::current_dir()
        .map_err(|error| format!("Could not read the current directory: {error}"))?;
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let source_switcher = git_source_switcher(&options, &directory)?;
    let input = execute_options(options.clone(), &directory, &mut stdin)?;
    drop(stdin);
    let watch = if matches!(&options.command, Command::Diff(options) if options.watch) {
        Some(
            WatchRequest::new(options, &directory, source_switcher.clone())
                .map_err(|error| format!("Could not watch diff: {error}"))?,
        )
    } else {
        None
    };
    Ok(Launch {
        input,
        watch,
        source_switcher,
    })
}

impl WatchRequest {
    fn new(
        options: Options,
        directory: &Path,
        source_switcher: Option<GitDiffSourceSwitcher>,
    ) -> Result<Self, String> {
        let repository_directory = options.repo.as_deref().map_or_else(
            || directory.to_owned(),
            |path| resolve_repo_path(directory, path),
        );
        let paths = match &options.command {
            Command::Diff(diff_options) => direct_file_paths(diff_options, &repository_directory)
                .map_or_else(
                || watch_paths(&repository_directory),
                |paths| Ok(paths.to_vec()),
            )?,
            Command::Show(_) | Command::StashShow(_) => watch_paths(&repository_directory)?,
            Command::Patch(_) => return Err("--watch cannot be used with patch".into()),
        };
        let paths = if let Some(source_switcher) = &source_switcher {
            let mut paths = source_switcher
                .worktrees()
                .iter()
                .map(|worktree| watch_paths(&worktree.path))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            paths.sort();
            paths.dedup();
            paths
        } else {
            paths
        };
        let reload = source_switcher.map_or_else(
            || ReloadRequest::Options {
                options,
                directory: directory.to_owned(),
            },
            ReloadRequest::Git,
        );
        Ok(Self { reload, paths })
    }
}

fn git_source_switcher(
    options: &Options,
    directory: &Path,
) -> Result<Option<GitDiffSourceSwitcher>, String> {
    let repository_directory = options.repo.as_deref().map_or_else(
        || directory.to_owned(),
        |path| resolve_repo_path(directory, path),
    );
    let (source, include_untracked, pathspecs) = match &options.command {
        Command::Diff(diff_options) => {
            if direct_file_paths(diff_options, &repository_directory).is_some() {
                return Ok(None);
            }
            let target = diff_options.targets.first().cloned();
            let pathspecs = if diff_options.pathspecs.is_empty() && diff_options.targets.len() > 1 {
                diff_options.targets[1..].to_vec()
            } else {
                diff_options.pathspecs.clone()
            };
            let source = if diff_options.staged {
                GitDiffSource::Staged(target)
            } else if let Some(target) = target {
                GitDiffSource::Comparison(target)
            } else {
                GitDiffSource::Changes
            };
            (source, !diff_options.exclude_untracked, pathspecs)
        }
        Command::Show(options) => (
            GitDiffSource::Commit(options.target.clone()),
            true,
            options.pathspecs.clone(),
        ),
        Command::StashShow(options) => (
            GitDiffSource::Stash(options.reference.clone()),
            true,
            Vec::new(),
        ),
        Command::Patch(_) => return Ok(None),
    };
    let directory = worktree_root(&repository_directory)?;
    let worktrees = load_worktrees(&directory)?;
    Ok(Some(GitDiffSourceSwitcher {
        directory: Arc::new(Mutex::new(directory)),
        source: Arc::new(Mutex::new(source)),
        worktrees: Arc::new(worktrees),
        include_untracked,
        pathspecs,
    }))
}

fn execute_options(
    options: Options,
    directory: &Path,
    stdin: &mut dyn Read,
) -> Result<Input, String> {
    if options.repo.is_some() && matches!(&options.command, Command::Patch(_)) {
        return Err("--repo cannot be used with patch".into());
    }
    let repository_directory = options.repo.as_deref().map_or_else(
        || directory.to_owned(),
        |path| resolve_repo_path(directory, path),
    );
    execute(options.command, &repository_directory, stdin)
}

fn execute(command: Command, directory: &Path, stdin: &mut dyn Read) -> Result<Input, String> {
    match command {
        Command::Diff(options) => execute_diff(&options, directory),
        Command::Show(ShowOptions { target, pathspecs }) => {
            load_show(directory, target.as_deref(), &pathspecs)
        }
        Command::StashShow(StashShowOptions { reference }) => {
            load_stash(directory, reference.as_deref())
        }
        Command::Patch(options) => execute_patch(options, directory, stdin),
    }
}

fn execute_diff(options: &DiffOptions, directory: &Path) -> Result<Input, String> {
    if let Some([before, after]) = direct_file_paths(options, directory) {
        return load_file_comparison(&before, &after);
    }

    if options.targets.len() > 1 && (options.staged || !options.pathspecs.is_empty()) {
        return Err(
            "diff accepts at most one Git target when --staged or pathspecs after -- are used"
                .into(),
        );
    }

    let target = options.targets.first().map(String::as_str);
    let pathspecs = if options.pathspecs.is_empty() && options.targets.len() > 1 {
        &options.targets[1..]
    } else {
        &options.pathspecs
    };
    load_diff(
        directory,
        &DiffRequest {
            target,
            staged: options.staged,
            include_untracked: !options.exclude_untracked,
            pathspecs,
        },
    )
}

fn direct_file_paths(options: &DiffOptions, directory: &Path) -> Option<[PathBuf; 2]> {
    let direct_files = options.targets.len() == 2
        && !options.staged
        && options.pathspecs.is_empty()
        && options
            .targets
            .iter()
            .map(|target| resolve_path(directory, target))
            .all(|path| path.is_file());
    direct_files.then(|| {
        [
            resolve_path(directory, &options.targets[0]),
            resolve_path(directory, &options.targets[1]),
        ]
    })
}

fn execute_patch(
    options: PatchOptions,
    directory: &Path,
    stdin: &mut dyn Read,
) -> Result<Input, String> {
    let (contents, source_name) = match options.file {
        Some(path) if path != Path::new("-") => {
            let path = if path.is_absolute() {
                path
            } else {
                directory.join(path)
            };
            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
            let source_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Local patch")
                .to_owned();
            (contents, source_name)
        }
        _ => {
            let mut contents = String::new();
            stdin
                .read_to_string(&mut contents)
                .map_err(|error| format!("Could not read patch from stdin: {error}"))?;
            (contents, "stdin".into())
        }
    };

    Ok(Input {
        diff: parse_unified_diff(contents),
        path_root: None,
        source_name,
        comparison_label: "before".into(),
        target_label: "after".into(),
        empty_title: "No changes in this patch".into(),
        empty_detail: "The patch contains no textual or metadata changes.".into(),
    })
}

fn load_file_comparison(before: &Path, after: &Path) -> Result<Input, String> {
    let before_contents = fs::read_to_string(before)
        .map_err(|error| format!("Could not open {}: {error}", before.display()))?;
    let after_contents = fs::read_to_string(after)
        .map_err(|error| format!("Could not open {}: {error}", after.display()))?;
    let before_label = before.to_string_lossy();
    let after_label = after.to_string_lossy();
    let patch = TextDiff::from_lines(&before_contents, &after_contents)
        .unified_diff()
        .context_radius(
            before_contents
                .lines()
                .count()
                .max(after_contents.lines().count()),
        )
        .header(&before_label, &after_label)
        .to_string();

    Ok(Input {
        diff: parse_unified_diff(patch),
        path_root: None,
        source_name: "File comparison".into(),
        comparison_label: file_label(before),
        target_label: file_label(after),
        empty_title: "Files are identical".into(),
        empty_detail: "No textual changes were found between the two files.".into(),
    })
}

fn resolve_path(directory: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_owned()
    } else {
        directory.join(path)
    }
}

fn resolve_repo_path(directory: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        directory.join(path)
    }
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_owned()
}
