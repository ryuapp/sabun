use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use git2::{IndexAddOption, Repository, Signature};

use super::{
    GitDiffSource, WatchRequest, execute, execute_options, git_source_switcher, repository,
};
use crate::cli::arguments::{
    Command, DiffOptions, Options, PatchOptions, ShowOptions, StashShowOptions, parse_from,
};
use crate::cli::repository::DiffRequest;

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

fn parse_command(args: &[&str]) -> Command {
    parse_from(args).unwrap().command
}

#[test]
fn cli_parses_diff_options_targets_and_strict_pathspecs() {
    assert_eq!(
        parse_command(&[
            "diff",
            "--staged",
            "--exclude-untracked",
            "HEAD",
            "--",
            "src",
            "README.md",
        ]),
        Command::Diff(DiffOptions {
            watch: false,
            staged: true,
            exclude_untracked: true,
            targets: vec!["HEAD".into()],
            pathspecs: vec!["src".into(), "README.md".into()],
        })
    );
    assert!(parse_from(&["diff", "--cached"]).is_err());
    assert_eq!(
        parse_command(&["diff", "--", "src"]),
        Command::Diff(DiffOptions {
            watch: false,
            staged: false,
            exclude_untracked: false,
            targets: Vec::new(),
            pathspecs: vec!["src".into()],
        })
    );
}

#[test]
fn cli_parses_show_stash_show_and_patch() {
    assert_eq!(
        parse_command(&["show", "HEAD~1", "--", "src/lib.rs"]),
        Command::Show(ShowOptions {
            target: Some("HEAD~1".into()),
            pathspecs: vec!["src/lib.rs".into()],
        })
    );
    assert_eq!(
        parse_command(&["stash", "show", "stash@{2}"]),
        Command::StashShow(StashShowOptions {
            reference: Some("stash@{2}".into()),
        })
    );
    assert_eq!(
        parse_command(&["stash", "show"]),
        Command::StashShow(StashShowOptions { reference: None })
    );
    assert_eq!(
        parse_command(&["patch", "-"]),
        Command::Patch(PatchOptions {
            file: Some("-".into()),
        })
    );
    assert_eq!(
        parse_command(&["patch"]),
        Command::Patch(PatchOptions { file: None })
    );
    assert_eq!(
        parse_command(&["show", "--", "src"]),
        Command::Show(ShowOptions {
            target: None,
            pathspecs: vec!["src".into()],
        })
    );
}

#[test]
fn cli_parses_watch_and_rejects_removed_demo_mode() {
    assert_eq!(
        parse_command(&["diff", "--watch"]),
        Command::Diff(DiffOptions {
            watch: true,
            staged: false,
            exclude_untracked: false,
            targets: Vec::new(),
            pathspecs: Vec::new(),
        })
    );
    assert!(parse_from(&["--demo"]).is_err());
}

#[test]
fn cli_parses_global_repo_selector() {
    let expected = Options {
        repo: Some("../nitro".into()),
        command: Command::Diff(DiffOptions {
            watch: false,
            staged: false,
            exclude_untracked: false,
            targets: Vec::new(),
            pathspecs: Vec::new(),
        }),
    };
    assert_eq!(
        parse_from(&["--repo", "../nitro", "diff"]).unwrap(),
        expected
    );
    assert_eq!(
        parse_from(&["diff", "--repo", "../nitro"]).unwrap(),
        expected
    );
}

#[test]
fn diff_combines_tracked_staged_and_untracked_changes() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("tracked.txt"), "working tree\n").unwrap();
    fs::write(fixture.root.join("staged.txt"), "staged\n").unwrap();
    stage(&fixture.root, "staged.txt");
    fs::write(fixture.root.join("untracked.txt"), "untracked\n").unwrap();

    let loaded = repository::load_diff(
        &fixture.root,
        &DiffRequest {
            target: None,
            staged: false,
            include_untracked: true,
            pathspecs: &[],
        },
    )
    .unwrap();
    assert_paths(
        &loaded.diff.files,
        &["staged.txt", "tracked.txt", "untracked.txt"],
    );
    assert_eq!(loaded.target_label, "working tree");
    assert_eq!(loaded.path_root.as_deref(), Some(fixture.root.as_path()));
}

#[test]
fn git_diff_loads_distant_unchanged_lines_only_when_expanded() {
    let fixture = Fixture::new();
    let lines = (1..=30)
        .map(|number| format!("line {number}"))
        .collect::<Vec<_>>();
    fs::write(fixture.root.join("tracked.txt"), lines.join("\n") + "\n").unwrap();
    fixture.commit_all("long file");
    let mut changed = lines;
    changed[1] = "changed near start".into();
    changed[28] = "changed near end".into();
    fs::write(fixture.root.join("tracked.txt"), changed.join("\n") + "\n").unwrap();

    let mut loaded = repository::load_diff(
        &fixture.root,
        &DiffRequest {
            target: None,
            staged: false,
            include_untracked: true,
            pathspecs: &[],
        },
    )
    .unwrap();
    assert_eq!(loaded.diff.files[0].hunks.len(), 2);
    assert_eq!(loaded.diff.old_line_count(0), Some(30));
    assert!(
        loaded.diff.files[0]
            .hunks
            .iter()
            .map(|hunk| hunk.lines.len())
            .sum::<usize>()
            < 15
    );

    assert!(loaded.diff.insert_context(0, 0, false, 6, 6, 20));
    let first_hunk = &loaded.diff.files[0].hunks[0];
    let expanded = &first_hunk.lines[first_hunk.lines.len() - 20..];
    assert_eq!(expanded.first().unwrap().old_number, Some(6));
    assert_eq!(expanded.last().unwrap().old_number, Some(25));
    assert_eq!(first_hunk.line_content(expanded.first().unwrap()), "line 6");
    assert_eq!(first_hunk.line_content(expanded.last().unwrap()), "line 25");
}

#[test]
fn git_diff_keeps_hunks_attached_to_their_files() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("second.txt"), "second initial\n").unwrap();
    fs::write(fixture.root.join("rename.txt"), "rename only\n").unwrap();
    fixture.commit_all("add files");
    fs::rename(
        fixture.root.join("rename.txt"),
        fixture.root.join("renamed.txt"),
    )
    .unwrap();
    let repository = Repository::open(&fixture.root).unwrap();
    let mut index = repository.index().unwrap();
    index.remove_path(Path::new("rename.txt")).unwrap();
    index.add_path(Path::new("renamed.txt")).unwrap();
    index.write().unwrap();
    fs::write(fixture.root.join("tracked.txt"), "first changed\n").unwrap();
    fs::write(fixture.root.join("second.txt"), "second changed\n").unwrap();

    let loaded = repository::load_diff(
        &fixture.root,
        &DiffRequest {
            target: None,
            staged: false,
            include_untracked: true,
            pathspecs: &[],
        },
    )
    .unwrap();

    let changed_lines = loaded
        .diff
        .files
        .iter()
        .map(|file| {
            let additions = file
                .hunks
                .iter()
                .flat_map(|hunk| {
                    hunk.lines
                        .iter()
                        .filter(|line| line.kind == crate::diff::LineKind::Addition)
                        .map(|line| hunk.line_content(line))
                })
                .collect::<Vec<_>>();
            (file.display_path(), additions)
        })
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(changed_lines["tracked.txt"], ["first changed"]);
    assert_eq!(changed_lines["second.txt"], ["second changed"]);
    assert!(changed_lines["renamed.txt"].is_empty());
}

#[test]
fn repo_selector_loads_a_different_repository() {
    let current = Fixture::new();
    let selected = Fixture::new();
    fs::write(selected.root.join("tracked.txt"), "selected working tree\n").unwrap();
    let mut stdin = Cursor::new(Vec::<u8>::new());

    let loaded = execute_options(
        Options {
            repo: Some(selected.root.clone()),
            command: Command::Diff(DiffOptions {
                watch: false,
                staged: false,
                exclude_untracked: false,
                targets: Vec::new(),
                pathspecs: Vec::new(),
            }),
        },
        &current.root,
        &mut stdin,
    )
    .unwrap();

    assert_paths(&loaded.diff.files, &["tracked.txt"]);
    assert_eq!(loaded.target_label, "working tree");
}

#[test]
fn repo_selector_is_rejected_for_patch_input() {
    let fixture = Fixture::new();
    let mut stdin = Cursor::new(Vec::<u8>::new());
    let error = execute_options(
        Options {
            repo: Some(fixture.root.clone()),
            command: Command::Patch(PatchOptions { file: None }),
        },
        &fixture.root,
        &mut stdin,
    )
    .err()
    .unwrap();
    assert_eq!(error, "--repo cannot be used with patch");
}

#[test]
fn watch_request_reloads_repository_diff() {
    let fixture = Fixture::new();
    let watch = WatchRequest::new(
        Options {
            repo: None,
            command: Command::Diff(DiffOptions {
                watch: true,
                staged: false,
                exclude_untracked: false,
                targets: Vec::new(),
                pathspecs: Vec::new(),
            }),
        },
        &fixture.root,
        None,
    )
    .unwrap();
    assert_eq!(watch.paths(), [fixture.root.as_path()]);

    fs::write(fixture.root.join("tracked.txt"), "watched change\n").unwrap();
    let loaded = watch.reload().unwrap();
    assert_paths(&loaded.diff.files, &["tracked.txt"]);
}

#[test]
fn git_source_switcher_loads_changes_staged_commit_and_stash() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("tracked.txt"), "working tree\n").unwrap();
    fs::write(fixture.root.join("staged.txt"), "staged\n").unwrap();
    stage(&fixture.root, "staged.txt");
    fs::write(fixture.root.join("stash.txt"), "stash\n").unwrap();
    stage(&fixture.root, "stash.txt");
    let mut repository = Repository::open(&fixture.root).unwrap();
    repository
        .stash_save(&signature(), "test stash", None)
        .unwrap();
    fs::write(fixture.root.join("tracked.txt"), "working tree again\n").unwrap();

    let options = Options {
        repo: None,
        command: Command::Diff(DiffOptions {
            watch: false,
            staged: false,
            exclude_untracked: false,
            targets: Vec::new(),
            pathspecs: Vec::new(),
        }),
    };
    let switcher = git_source_switcher(&options, &fixture.root)
        .unwrap()
        .unwrap();
    assert_eq!(switcher.source(), GitDiffSource::Changes);
    let catalog = switcher.catalog().unwrap();
    assert_eq!(catalog.commits.len(), 1);
    assert!(!catalog.has_more_commits);
    assert_eq!(catalog.commits[0].summary, "initial");
    assert_eq!(catalog.stashes.len(), 1);
    assert_eq!(catalog.stashes[0].reference, "stash@{0}");
    assert_paths(
        &switcher
            .switch_to(GitDiffSource::Staged(None))
            .unwrap()
            .diff
            .files,
        &[],
    );
    assert_eq!(switcher.source(), GitDiffSource::Staged(None));
    assert_paths(
        &switcher
            .switch_to(GitDiffSource::Commit(None))
            .unwrap()
            .diff
            .files,
        &["tracked.txt"],
    );
    assert_paths(
        &switcher
            .switch_to(GitDiffSource::Stash(None))
            .unwrap()
            .diff
            .files,
        &["staged.txt", "stash.txt", "tracked.txt"],
    );
    assert_paths(
        &switcher
            .switch_to(GitDiffSource::Changes)
            .unwrap()
            .diff
            .files,
        &["tracked.txt"],
    );
}

#[test]
fn git_source_switcher_discovers_and_switches_linked_worktrees() {
    let fixture = Fixture::new();
    let linked = fixture.add_worktree("linked");
    fs::write(linked.join("tracked.txt"), "linked worktree change\n").unwrap();

    let options = Options {
        repo: None,
        command: Command::Diff(DiffOptions {
            watch: false,
            staged: false,
            exclude_untracked: false,
            targets: Vec::new(),
            pathspecs: Vec::new(),
        }),
    };
    let switcher = git_source_switcher(&options, &fixture.root)
        .unwrap()
        .unwrap();

    assert_eq!(switcher.worktrees().len(), 2);
    assert!(
        switcher
            .worktrees()
            .iter()
            .any(|worktree| worktree.path == fixture.root)
    );
    assert!(
        switcher
            .worktrees()
            .iter()
            .any(|worktree| worktree.path == linked && worktree.branch == "linked")
    );

    let input = switcher.switch_worktree(linked.clone()).unwrap();
    assert_eq!(switcher.source(), GitDiffSource::Changes);
    assert_eq!(switcher.current_worktree().unwrap().path, linked.as_path());
    assert_eq!(input.path_root.as_deref(), Some(linked.as_path()));
    assert_paths(&input.diff.files, &["tracked.txt"]);
}

#[test]
fn commit_history_loads_in_pages() {
    let fixture = Fixture::new();
    for index in 0..55 {
        fs::write(
            fixture.root.join("tracked.txt"),
            format!("history {index}\n"),
        )
        .unwrap();
        fixture.commit_all(&format!("history {index}"));
    }

    let first = repository::load_commit_page(&fixture.root, 0).unwrap();
    assert_eq!(first.commits.len(), 50);
    assert!(first.has_more);
    assert_eq!(first.commits.first().unwrap().summary, "history 54");
    assert_eq!(first.commits.last().unwrap().summary, "history 5");

    let second = repository::load_commit_page(&fixture.root, first.commits.len()).unwrap();
    assert_eq!(second.commits.len(), 6);
    assert!(!second.has_more);
    assert_eq!(second.commits.first().unwrap().summary, "history 4");
    assert_eq!(second.commits.last().unwrap().summary, "initial");
}

#[test]
fn diff_supports_staged_excluding_untracked_and_pathspecs() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("tracked.txt"), "working tree\n").unwrap();
    fs::write(fixture.root.join("staged.txt"), "staged\n").unwrap();
    stage(&fixture.root, "staged.txt");
    fs::write(fixture.root.join("untracked.txt"), "untracked\n").unwrap();

    let staged = repository::load_diff(
        &fixture.root,
        &DiffRequest {
            target: None,
            staged: true,
            include_untracked: true,
            pathspecs: &[],
        },
    )
    .unwrap();
    assert_paths(&staged.diff.files, &["staged.txt"]);
    assert_eq!(staged.target_label, "index");

    let without_untracked = repository::load_diff(
        &fixture.root,
        &DiffRequest {
            target: None,
            staged: false,
            include_untracked: false,
            pathspecs: &[],
        },
    )
    .unwrap();
    assert_paths(
        &without_untracked.diff.files,
        &["staged.txt", "tracked.txt"],
    );

    let tracked_only = repository::load_diff(
        &fixture.root,
        &DiffRequest {
            target: None,
            staged: false,
            include_untracked: true,
            pathspecs: &["tracked.txt".into()],
        },
    )
    .unwrap();
    assert_paths(&tracked_only.diff.files, &["tracked.txt"]);
}

#[test]
fn diff_supports_revision_ranges() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("tracked.txt"), "second commit\n").unwrap();
    fixture.commit_all("second");

    let loaded = repository::load_diff(
        &fixture.root,
        &DiffRequest {
            target: Some("HEAD~1..HEAD"),
            staged: false,
            include_untracked: true,
            pathspecs: &[],
        },
    )
    .unwrap();
    assert_paths(&loaded.diff.files, &["tracked.txt"]);
    assert_eq!(loaded.comparison_label, "HEAD~1");
    assert_eq!(loaded.target_label, "HEAD");

    let merge_base = repository::load_diff(
        &fixture.root,
        &DiffRequest {
            target: Some("HEAD~1...HEAD"),
            staged: false,
            include_untracked: true,
            pathspecs: &[],
        },
    )
    .unwrap();
    assert_paths(&merge_base.diff.files, &["tracked.txt"]);
}

#[test]
fn diff_supports_a_single_revision_against_the_working_tree() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("tracked.txt"), "second commit\n").unwrap();
    fixture.commit_all("second");
    fs::write(fixture.root.join("tracked.txt"), "working tree\n").unwrap();

    let loaded = repository::load_diff(
        &fixture.root,
        &DiffRequest {
            target: Some("HEAD~1"),
            staged: false,
            include_untracked: true,
            pathspecs: &[],
        },
    )
    .unwrap();
    assert_paths(&loaded.diff.files, &["tracked.txt"]);
    assert_eq!(loaded.comparison_label, "HEAD~1");
    assert_eq!(loaded.target_label, "working tree");
}

#[test]
fn show_defaults_to_head_and_filters_by_pathspec() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("tracked.txt"), "second commit\n").unwrap();
    fs::write(fixture.root.join("other.txt"), "other\n").unwrap();
    fixture.commit_all("second");

    let loaded = repository::load_show(&fixture.root, None, &["tracked.txt".into()]).unwrap();
    assert_paths(&loaded.diff.files, &["tracked.txt"]);
    assert_eq!(loaded.target_label, "HEAD");
}

#[test]
fn show_supports_an_explicit_root_commit() {
    let fixture = Fixture::new();
    let loaded = repository::load_show(&fixture.root, Some("HEAD"), &[]).unwrap();
    assert_paths(&loaded.diff.files, &["tracked.txt"]);
    assert_eq!(loaded.comparison_label, "empty tree");
}

#[test]
fn stash_show_reads_the_selected_stash_through_libgit2() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("tracked.txt"), "stashed change\n").unwrap();
    let mut repository = Repository::open(&fixture.root).unwrap();
    let signature = signature();
    repository
        .stash_save(&signature, "test stash", None)
        .unwrap();

    let loaded = repository::load_stash(&fixture.root, Some("stash@{0}")).unwrap();
    assert_paths(&loaded.diff.files, &["tracked.txt"]);
    assert_eq!(loaded.target_label, "stash@{0}");

    let default_stash = repository::load_stash(&fixture.root, None).unwrap();
    assert_eq!(default_stash.diff, loaded.diff);
}

#[test]
fn patch_reads_stdin_and_file_comparison_uses_two_existing_files() {
    let fixture = Fixture::new();
    let patch = b"--- a/old.txt\n+++ b/old.txt\n@@ -1 +1 @@\n-before\n+after\n";
    let mut stdin = Cursor::new(patch);
    let loaded = execute(
        Command::Patch(PatchOptions { file: None }),
        &fixture.root,
        &mut stdin,
    )
    .unwrap();
    assert_paths(&loaded.diff.files, &["old.txt"]);
    assert_eq!(loaded.source_name, "stdin");
    assert!(loaded.path_root.is_none());

    let mut stdin = Cursor::new(patch);
    let loaded = execute(
        Command::Patch(PatchOptions {
            file: Some("-".into()),
        }),
        &fixture.root,
        &mut stdin,
    )
    .unwrap();
    assert_paths(&loaded.diff.files, &["old.txt"]);
    assert_eq!(loaded.source_name, "stdin");

    let patch_file = fixture.root.join("changes.patch");
    fs::write(&patch_file, patch).unwrap();
    let mut empty_stdin = Cursor::new(Vec::<u8>::new());
    let loaded = execute(
        Command::Patch(PatchOptions {
            file: Some(patch_file),
        }),
        &fixture.root,
        &mut empty_stdin,
    )
    .unwrap();
    assert_paths(&loaded.diff.files, &["old.txt"]);
    assert_eq!(loaded.source_name, "changes.patch");

    let before = fixture.root.join("before.txt");
    let after = fixture.root.join("after.txt");
    fs::write(&before, "before\n").unwrap();
    fs::write(&after, "after\n").unwrap();
    let mut empty_stdin = Cursor::new(Vec::<u8>::new());
    let loaded = execute(
        Command::Diff(DiffOptions {
            watch: false,
            staged: false,
            exclude_untracked: false,
            targets: vec![
                before.to_string_lossy().into(),
                after.to_string_lossy().into(),
            ],
            pathspecs: Vec::new(),
        }),
        &fixture.root,
        &mut empty_stdin,
    )
    .unwrap();
    assert_eq!(loaded.diff.files.len(), 1);
    assert_eq!(loaded.comparison_label, "before.txt");
    assert_eq!(loaded.target_label, "after.txt");
}

fn assert_paths(files: &[crate::diff::DiffFile], expected: &[&str]) {
    let mut actual = files
        .iter()
        .map(|file| file.display_path().to_owned())
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = expected
        .iter()
        .map(|path| (*path).to_owned())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(actual, expected);
}

struct Fixture {
    root: PathBuf,
    cleanup_root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let fixture_id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let cleanup_root = std::env::temp_dir().join(format!(
            "sabun-git-test-{}-{fixture_id}",
            std::process::id()
        ));
        let root = cleanup_root.join("main");
        fs::create_dir_all(&root).unwrap();
        let repository = Repository::init(&root).unwrap();
        let root = repository.workdir().unwrap().to_owned();
        fs::write(root.join("tracked.txt"), "initial\n").unwrap();
        commit_all(&repository, "initial");
        Self { root, cleanup_root }
    }

    fn commit_all(&self, message: &str) {
        let repository = Repository::open(&self.root).unwrap();
        commit_all(&repository, message);
    }

    fn add_worktree(&self, name: &str) -> PathBuf {
        let path = self.cleanup_root.join(name);
        let repository = Repository::open(&self.root).unwrap();
        repository.worktree(name, &path, None).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.cleanup_root);
    }
}

fn commit_all(repository: &Repository, message: &str) {
    let mut index = repository.index().unwrap();
    index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repository.find_tree(tree_id).unwrap();
    let signature = signature();
    let parents = repository
        .head()
        .ok()
        .and_then(|head| head.target())
        .map(|oid| repository.find_commit(oid).unwrap());
    let parents = parents.iter().collect::<Vec<_>>();
    repository
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .unwrap();
}

fn stage(root: &Path, path: &str) {
    let repository = Repository::open(root).unwrap();
    let mut index = repository.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

fn signature() -> Signature<'static> {
    Signature::now("sabun test", "sabun@example.invalid").unwrap()
}
