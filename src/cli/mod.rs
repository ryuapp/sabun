mod arguments;
mod repository;
#[cfg(test)]
mod tests;

use std::{
    fs,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
};

use similar::TextDiff;

use crate::diff::{DiffSet, parse_unified_diff};
use arguments::{
    Command, DiffOptions, Options, PatchOptions, ShowOptions, StashShowOptions, parse,
};
use repository::{DiffRequest, load_diff, load_show, load_stash, watch_paths};

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
}

#[derive(Clone)]
pub(super) struct WatchRequest {
    options: Options,
    directory: PathBuf,
    paths: Vec<PathBuf>,
}

impl WatchRequest {
    pub(super) fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub(super) fn reload(&self) -> Result<Input, String> {
        execute_options(
            self.options.clone(),
            &self.directory,
            &mut Cursor::new(Vec::new()),
        )
    }
}

pub(super) fn load() -> Result<Launch, String> {
    let options = parse();
    let directory = std::env::current_dir()
        .map_err(|error| format!("Could not read the current directory: {error}"))?;
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let input = execute_options(options.clone(), &directory, &mut stdin)?;
    drop(stdin);
    let watch = if matches!(&options.command, Command::Diff(options) if options.watch) {
        Some(
            WatchRequest::new(options, &directory)
                .map_err(|error| format!("Could not watch diff: {error}"))?,
        )
    } else {
        None
    };
    Ok(Launch { input, watch })
}

impl WatchRequest {
    fn new(options: Options, directory: &Path) -> Result<Self, String> {
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
        Ok(Self {
            options,
            directory: directory.to_owned(),
            paths,
        })
    }
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
