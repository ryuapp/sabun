use std::path::{Path, PathBuf};

use git2::{Diff, DiffFindOptions, DiffOptions, ErrorCode, Object, Oid, Repository, Sort, Tree};

use super::{GitCommitPage, GitCommitSource, GitSourceCatalog, GitStashSource, GitWorktree, Input};
use crate::diff::{DiffSet, from_git_diff};

const DIFF_CONTEXT_LINES: u32 = 3;
const COMMIT_HISTORY_PAGE_SIZE: usize = 50;

pub(super) struct DiffRequest<'a> {
    pub(super) target: Option<&'a str>,
    pub(super) staged: bool,
    pub(super) include_untracked: bool,
    pub(super) pathspecs: &'a [String],
}

pub(super) fn load_diff(start: &Path, request: &DiffRequest<'_>) -> Result<Input, String> {
    let repository = discover(start)?;
    let source_name = repository_name(&repository);
    let mut options = diff_options(
        request.include_untracked && !request.staged,
        request.pathspecs,
    );

    let (diff, base_label, target_label) = if request.staged {
        let base_tree = match request.target {
            Some(target) => Some(resolve_tree(&repository, target)?),
            None => head_tree(&repository)?,
        };
        let diff = repository
            .diff_tree_to_index(base_tree.as_ref(), None, Some(&mut options))
            .map_err(|error| format!("Could not read staged changes: {error}"))?;
        let base_label = request
            .target
            .map_or_else(|| branch_label(&repository), str::to_owned);
        (diff, base_label, "index".to_owned())
    } else if let Some(target) = request.target {
        let revspec = repository
            .revparse(target)
            .map_err(|error| format!("Could not resolve {target}: {error}"))?;
        if revspec.mode().is_range() {
            let from = revspec
                .from()
                .ok_or_else(|| format!("Range {target} has no left revision"))?;
            let to = revspec
                .to()
                .ok_or_else(|| format!("Range {target} has no right revision"))?;
            let from_tree;
            let to_tree = peel_tree(to, target)?;
            if revspec.mode().is_merge_base() {
                let from_commit = from.peel_to_commit().map_err(|error| {
                    format!("Could not resolve the left commit in {target}: {error}")
                })?;
                let to_commit = to.peel_to_commit().map_err(|error| {
                    format!("Could not resolve the right commit in {target}: {error}")
                })?;
                let merge_base = repository
                    .merge_base(from_commit.id(), to_commit.id())
                    .map_err(|error| {
                        format!("Could not find the merge base for {target}: {error}")
                    })?;
                from_tree = repository
                    .find_commit(merge_base)
                    .and_then(|commit| commit.tree())
                    .map_err(|error| {
                        format!("Could not read the merge base for {target}: {error}")
                    })?;
            } else {
                from_tree = peel_tree(from, target)?;
            }
            let diff = repository
                .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut options))
                .map_err(|error| format!("Could not compare {target}: {error}"))?;
            let (base_label, target_label) = range_labels(target);
            (diff, base_label, target_label)
        } else {
            let base_tree = revspec
                .from()
                .or_else(|| revspec.to())
                .ok_or_else(|| format!("Revision {target} did not resolve to an object"))
                .and_then(|object| peel_tree(object, target))?;
            let diff = repository
                .diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut options))
                .map_err(|error| {
                    format!("Could not compare {target} to the working tree: {error}")
                })?;
            (diff, target.to_owned(), "working tree".to_owned())
        }
    } else {
        let base_tree = head_tree(&repository)?;
        let diff = repository
            .diff_tree_to_workdir_with_index(base_tree.as_ref(), Some(&mut options))
            .map_err(|error| format!("Could not read working-tree changes: {error}"))?;
        (diff, branch_label(&repository), "working tree".to_owned())
    };

    Ok(Input {
        diff: finish_diff(&repository, diff)?,
        path_root: repository.workdir().map(Path::to_path_buf),
        source_name,
        comparison_label: base_label,
        target_label,
        empty_title: if request.staged {
            "No staged changes".into()
        } else {
            "Working tree is clean".into()
        },
        empty_detail: if request.staged {
            "The index contains no changes for this comparison.".into()
        } else {
            "No matching textual or metadata changes were found.".into()
        },
    })
}

pub(super) fn load_show(
    start: &Path,
    target: Option<&str>,
    pathspecs: &[String],
) -> Result<Input, String> {
    let repository = discover(start)?;
    let requested = target.unwrap_or("HEAD");
    let commit = repository
        .revparse_single(requested)
        .and_then(|object| object.peel_to_commit())
        .map_err(|error| format!("Could not resolve commit {requested}: {error}"))?;
    let new_tree = commit
        .tree()
        .map_err(|error| format!("Could not read commit {requested}: {error}"))?;
    let old_tree = if commit.parent_count() == 0 {
        None
    } else {
        Some(
            commit
                .parent(0)
                .and_then(|parent| parent.tree())
                .map_err(|error| format!("Could not read the parent of {requested}: {error}"))?,
        )
    };
    let mut options = diff_options(false, pathspecs);
    let diff = repository
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut options))
        .map_err(|error| format!("Could not show commit {requested}: {error}"))?;

    Ok(Input {
        diff: finish_diff(&repository, diff)?,
        path_root: repository.workdir().map(Path::to_path_buf),
        source_name: repository_name(&repository),
        comparison_label: old_tree
            .as_ref()
            .map_or_else(|| "empty tree".into(), |tree| short_oid(tree.id())),
        target_label: requested.to_owned(),
        empty_title: "Commit has no matching changes".into(),
        empty_detail: "No textual or metadata changes matched this comparison.".into(),
    })
}

pub(super) fn load_stash(start: &Path, reference: Option<&str>) -> Result<Input, String> {
    let mut repository = discover(start)?;
    let requested = reference.unwrap_or("stash@{0}");
    let stash_oid = resolve_stash(&mut repository, requested)?;
    let stash = repository
        .find_commit(stash_oid)
        .map_err(|error| format!("Could not read {requested}: {error}"))?;
    let base = stash
        .parent(0)
        .map_err(|error| format!("Could not read the base commit for {requested}: {error}"))?;
    let base_tree = base
        .tree()
        .map_err(|error| format!("Could not read the base tree for {requested}: {error}"))?;
    let stash_tree = stash
        .tree()
        .map_err(|error| format!("Could not read the saved tree for {requested}: {error}"))?;
    let mut options = diff_options(false, &[]);
    let diff = repository
        .diff_tree_to_tree(Some(&base_tree), Some(&stash_tree), Some(&mut options))
        .map_err(|error| format!("Could not show {requested}: {error}"))?;

    Ok(Input {
        diff: finish_diff(&repository, diff)?,
        path_root: repository.workdir().map(Path::to_path_buf),
        source_name: repository_name(&repository),
        comparison_label: short_oid(base.id()),
        target_label: requested.to_owned(),
        empty_title: "Stash has no changes".into(),
        empty_detail: "The selected stash contains no textual or metadata changes.".into(),
    })
}

fn discover(start: &Path) -> Result<Repository, String> {
    Repository::discover(start).map_err(|error| {
        format!(
            "Could not find a Git repository from {}: {error}",
            start.display()
        )
    })
}

pub(super) fn worktree_root(start: &Path) -> Result<PathBuf, String> {
    let repository = discover(start)?;
    Ok(repository
        .workdir()
        .unwrap_or_else(|| repository.path())
        .to_owned())
}

pub(super) fn load_worktrees(start: &Path) -> Result<Vec<GitWorktree>, String> {
    let repository = discover(start)?;
    let common_repository = Repository::open(repository.commondir()).map_err(|error| {
        format!(
            "Could not open the shared Git directory {}: {error}",
            repository.commondir().display()
        )
    })?;
    let mut paths = Vec::new();
    if let Some(path) = common_repository.workdir() {
        paths.push(path.to_owned());
    }

    let names = common_repository
        .worktrees()
        .map_err(|error| format!("Could not enumerate Git worktrees: {error}"))?;
    for name in &names {
        let name = name.map_err(|error| format!("Could not decode a worktree name: {error}"))?;
        let Some(name) = name else {
            continue;
        };
        let worktree = common_repository
            .find_worktree(name)
            .map_err(|error| format!("Could not read worktree {name}: {error}"))?;
        let path = worktree.path().to_owned();
        if !paths.contains(&path) {
            paths.push(path);
        }
    }

    paths
        .into_iter()
        .map(|path| {
            let worktree_repository = discover(&path)?;
            Ok(GitWorktree {
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("worktree")
                    .to_owned(),
                branch: branch_label(&worktree_repository),
                path,
            })
        })
        .collect()
}

pub(super) fn watch_paths(start: &Path) -> Result<Vec<PathBuf>, String> {
    let repository = discover(start)?;
    let worktree = repository
        .workdir()
        .unwrap_or_else(|| repository.path())
        .to_owned();
    let git_dir = repository.path().to_owned();
    let mut paths = vec![worktree.clone()];
    if !git_dir.starts_with(&worktree) {
        paths.push(git_dir);
    }
    let common_dir = repository.commondir().to_owned();
    if !common_dir.starts_with(&worktree) && !paths.contains(&common_dir) {
        paths.push(common_dir);
    }
    Ok(paths)
}

pub(super) fn load_source_catalog(start: &Path) -> Result<GitSourceCatalog, String> {
    let mut repository = discover(start)?;
    let GitCommitPage { commits, has_more } = commit_page(&repository, 0)?;

    let mut stashes = Vec::new();
    repository
        .stash_foreach(|index, message, oid| {
            stashes.push(GitStashSource {
                reference: format!("stash@{{{index}}}"),
                short_oid: short_oid(*oid),
                summary: message.to_owned(),
            });
            true
        })
        .map_err(|error| format!("Could not read stash history: {error}"))?;

    Ok(GitSourceCatalog {
        commits,
        has_more_commits: has_more,
        stashes,
    })
}

pub(super) fn load_commit_page(start: &Path, offset: usize) -> Result<GitCommitPage, String> {
    let repository = discover(start)?;
    commit_page(&repository, offset)
}

fn commit_page(repository: &Repository, offset: usize) -> Result<GitCommitPage, String> {
    let mut commits = Vec::new();
    let mut revwalk = repository
        .revwalk()
        .map_err(|error| format!("Could not read commit history: {error}"))?;
    match revwalk.push_head() {
        Ok(()) => {
            revwalk
                .set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
                .map_err(|error| format!("Could not sort commit history: {error}"))?;
            for oid in revwalk.skip(offset).take(COMMIT_HISTORY_PAGE_SIZE + 1) {
                let oid = oid.map_err(|error| format!("Could not walk commit history: {error}"))?;
                let commit = repository
                    .find_commit(oid)
                    .map_err(|error| format!("Could not read commit {oid}: {error}"))?;
                commits.push(GitCommitSource {
                    oid: oid.to_string(),
                    short_oid: short_oid(oid),
                    summary: commit
                        .summary()
                        .ok()
                        .flatten()
                        .unwrap_or("Untitled commit")
                        .to_owned(),
                    author: commit
                        .author()
                        .name()
                        .unwrap_or("Unknown author")
                        .to_owned(),
                });
            }
        }
        Err(error) if matches!(error.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {}
        Err(error) => return Err(format!("Could not read HEAD: {error}")),
    }

    let has_more = commits.len() > COMMIT_HISTORY_PAGE_SIZE;
    commits.truncate(COMMIT_HISTORY_PAGE_SIZE);
    Ok(GitCommitPage { commits, has_more })
}

fn diff_options(include_untracked: bool, pathspecs: &[String]) -> DiffOptions {
    let mut options = DiffOptions::new();
    options
        .include_untracked(include_untracked)
        .recurse_untracked_dirs(include_untracked)
        .show_untracked_content(include_untracked)
        .include_typechange(true)
        .include_typechange_trees(true)
        .ignore_submodules(true)
        .context_lines(DIFF_CONTEXT_LINES)
        .interhunk_lines(0)
        .max_size(2 * 1024 * 1024);
    for pathspec in pathspecs {
        options.pathspec(pathspec);
    }
    options
}

fn finish_diff(repository: &Repository, mut diff: Diff<'_>) -> Result<DiffSet, String> {
    let mut find_options = DiffFindOptions::new();
    find_options.renames(true);
    diff.find_similar(Some(&mut find_options))
        .map_err(|error| format!("Could not detect renamed files: {error}"))?;
    from_git_diff(repository, &diff)
        .map_err(|error| format!("Could not build the diff view: {error}"))
}

fn resolve_tree<'repo>(repository: &'repo Repository, target: &str) -> Result<Tree<'repo>, String> {
    repository
        .revparse_single(target)
        .map_err(|error| format!("Could not resolve {target}: {error}"))
        .and_then(|object| peel_tree(&object, target))
}

fn peel_tree<'repo>(object: &Object<'repo>, target: &str) -> Result<Tree<'repo>, String> {
    object
        .peel_to_tree()
        .map_err(|error| format!("{target} does not resolve to a tree: {error}"))
}

fn head_tree(repository: &Repository) -> Result<Option<Tree<'_>>, String> {
    match repository.head() {
        Ok(head) => head
            .peel_to_tree()
            .map(Some)
            .map_err(|error| format!("Could not read HEAD: {error}")),
        Err(error) if matches!(error.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
            Ok(None)
        }
        Err(error) => Err(format!("Could not read HEAD: {error}")),
    }
}

fn resolve_stash(repository: &mut Repository, reference: &str) -> Result<Oid, String> {
    if let Some(index) = stash_index(reference) {
        let mut found = None;
        repository
            .stash_foreach(|candidate, _, oid| {
                if candidate == index {
                    found = Some(*oid);
                    false
                } else {
                    true
                }
            })
            .map_err(|error| format!("Could not enumerate stashes: {error}"))?;
        return found.ok_or_else(|| format!("Stash {reference} does not exist"));
    }

    repository
        .revparse_single(reference)
        .map(|object| object.id())
        .map_err(|error| format!("Could not resolve stash {reference}: {error}"))
}

fn stash_index(reference: &str) -> Option<usize> {
    reference
        .strip_prefix("stash@{")?
        .strip_suffix('}')?
        .parse()
        .ok()
}

fn repository_name(repository: &Repository) -> String {
    repository
        .workdir()
        .or_else(|| repository.path().parent())
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("Git repository")
        .to_owned()
}

fn branch_label(repository: &Repository) -> String {
    repository
        .head()
        .ok()
        .and_then(|head| {
            head.shorthand()
                .ok()
                .map(str::to_owned)
                .or_else(|| head.target().map(short_oid))
        })
        .unwrap_or_else(|| "unborn branch".into())
}

fn range_labels(target: &str) -> (String, String) {
    target
        .split_once("...")
        .or_else(|| target.split_once(".."))
        .map_or_else(
            || (target.to_owned(), "range".into()),
            |(left, right)| (left.to_owned(), right.to_owned()),
        )
}

fn short_oid(oid: Oid) -> String {
    oid.to_string()[..7].to_owned()
}
