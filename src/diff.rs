use std::{
    cell::{Cell, RefCell},
    ops::Range,
    path::Path,
    sync::{Arc, OnceLock},
};

use git2::{Delta, Diff as GitDiff, DiffLineType, Repository};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineKind {
    Context,
    Addition,
    Deletion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffLine {
    pub old_number: Option<u32>,
    pub new_number: Option<u32>,
    pub kind: LineKind,
    content_range: Range<u32>,
}

impl DiffLine {
    pub fn content_from<'a>(&self, storage: &'a str) -> &'a str {
        &storage[self.content_range.start as usize..self.content_range.end as usize]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffHunk {
    pub header: Arc<str>,
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
    pub(crate) content: Arc<String>,
}

impl DiffHunk {
    pub fn line_content(&self, line: &DiffLine) -> &str {
        line.content_from(&self.content)
    }

    fn insert_context<I, S>(&mut self, at_start: bool, old_start: u32, new_start: u32, contents: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let content = Arc::make_mut(&mut self.content);
        let mut lines = Vec::new();
        for (offset, line_content) in contents.into_iter().enumerate() {
            let offset = u32::try_from(offset).expect("context expansion exceeds u32::MAX lines");
            let start = u32::try_from(content.len()).expect("diff hunk exceeds 4 GiB");
            content.push_str(line_content.as_ref());
            let end = u32::try_from(content.len()).expect("diff hunk exceeds 4 GiB");
            lines.push(DiffLine {
                old_number: Some(old_start.saturating_add(offset)),
                new_number: Some(new_start.saturating_add(offset)),
                kind: LineKind::Context,
                content_range: start..end,
            });
        }
        if at_start {
            self.old_start = old_start;
            self.new_start = new_start;
            lines.append(&mut self.lines);
            self.lines = lines;
        } else {
            self.lines.extend(lines);
        }
    }

    #[cfg(test)]
    pub(crate) fn from_lines<I, S>(header: &str, old_start: u32, new_start: u32, lines: I) -> Self
    where
        I: IntoIterator<Item = (Option<u32>, Option<u32>, LineKind, S)>,
        S: AsRef<str>,
    {
        let mut builder = DiffHunkBuilder::new(header, old_start, new_start);
        for (old_number, new_number, kind, content) in lines {
            builder.push_line(old_number, new_number, kind, content.as_ref());
        }
        builder.finish()
    }
}

struct DiffHunkBuilder {
    header: Arc<str>,
    old_start: u32,
    new_start: u32,
    lines: Vec<DiffLine>,
    content: String,
}

impl DiffHunkBuilder {
    fn new(header: impl Into<Arc<str>>, old_start: u32, new_start: u32) -> Self {
        Self {
            header: header.into(),
            old_start,
            new_start,
            lines: Vec::new(),
            content: String::new(),
        }
    }

    fn push_line(
        &mut self,
        old_number: Option<u32>,
        new_number: Option<u32>,
        kind: LineKind,
        content: &str,
    ) {
        let start = u32::try_from(self.content.len()).expect("diff hunk exceeds 4 GiB");
        self.content.push_str(content);
        let end = u32::try_from(self.content.len()).expect("diff hunk exceeds 4 GiB");
        self.lines.push(DiffLine {
            old_number,
            new_number,
            kind,
            content_range: start..end,
        });
    }

    fn finish(self) -> DiffHunk {
        DiffHunk {
            header: self.header,
            old_start: self.old_start,
            new_start: self.new_start,
            lines: self.lines,
            content: Arc::new(self.content),
        }
    }
}

struct SharedDiffHunkBuilder {
    header: Arc<str>,
    old_start: u32,
    new_start: u32,
    lines: Vec<DiffLine>,
}

impl SharedDiffHunkBuilder {
    fn new(header: &str, old_start: u32, new_start: u32) -> Self {
        Self {
            header: header.into(),
            old_start,
            new_start,
            lines: Vec::new(),
        }
    }

    fn push_line(
        &mut self,
        old_number: Option<u32>,
        new_number: Option<u32>,
        kind: LineKind,
        content_range: Range<u32>,
    ) {
        self.lines.push(DiffLine {
            old_number,
            new_number,
            kind,
            content_range,
        });
    }

    fn finish(self, content: Arc<String>) -> DiffHunk {
        DiffHunk {
            header: self.header,
            old_start: self.old_start,
            new_start: self.new_start,
            lines: self.lines,
            content,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffFile {
    pub old_path: String,
    pub new_path: String,
    pub hunks: Vec<DiffHunk>,
    pub is_new: bool,
    pub is_deleted: bool,
}

impl DiffFile {
    pub fn path(&self) -> &str {
        if self.new_path == "/dev/null" {
            &self.old_path
        } else {
            &self.new_path
        }
    }

    pub fn display_path(&self) -> &str {
        self.path()
            .strip_prefix("a/")
            .or_else(|| self.path().strip_prefix("b/"))
            .unwrap_or_else(|| self.path())
    }

    pub fn file_name(&self) -> &str {
        Path::new(self.display_path())
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| self.display_path())
    }

    pub fn parent_path(&self) -> &str {
        self.display_path()
            .strip_suffix(self.file_name())
            .unwrap_or("")
            .trim_end_matches(['/', '\\'])
    }

    pub fn additions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.kind == LineKind::Addition)
            .count()
    }

    pub fn deletions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.kind == LineKind::Deletion)
            .count()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiffSet {
    pub files: Vec<DiffFile>,
    contexts: Vec<Option<DiffFileContext>>,
}

impl DiffSet {
    #[cfg(test)]
    pub fn additions(&self) -> usize {
        self.files.iter().map(DiffFile::additions).sum()
    }

    #[cfg(test)]
    pub fn deletions(&self) -> usize {
        self.files.iter().map(DiffFile::deletions).sum()
    }

    pub(crate) fn old_line_count(&self, file_index: usize) -> Option<u32> {
        self.contexts
            .get(file_index)?
            .as_ref()
            .map(DiffFileContext::line_count)
    }

    pub(crate) fn syntax_source(&self, file_index: usize, old: bool) -> Option<Arc<String>> {
        // Full-file context is only needed while SyntaxMate is highlighting this file. Building
        // both sides for every file up front duplicates a large patch even if most files are
        // never visited, so streams request and own just the side they currently need.
        let file = self.files.get(file_index)?;
        if old {
            return self
                .contexts
                .get(file_index)
                .and_then(Option::as_ref)
                .map(|context| Arc::clone(&context.content))
                .or_else(|| complete_added_or_deleted_source(file, true));
        }
        if file.is_deleted {
            return Some(Arc::new(String::new()));
        }
        if file.is_new {
            return complete_added_or_deleted_source(file, false);
        }
        self.contexts
            .get(file_index)
            .and_then(Option::as_ref)
            .and_then(|context| patched_source(file, context))
            .map(Arc::new)
    }

    pub(crate) fn insert_context(
        &mut self,
        file_index: usize,
        hunk_index: usize,
        at_start: bool,
        old_start: u32,
        new_start: u32,
        count: usize,
    ) -> bool {
        let Some(context) = self.contexts.get(file_index).and_then(Option::as_ref) else {
            return false;
        };
        let Some(contents) = context.lines(old_start, count) else {
            return false;
        };
        let Some(hunk) = self
            .files
            .get_mut(file_index)
            .and_then(|file| file.hunks.get_mut(hunk_index))
        else {
            return false;
        };
        hunk.insert_context(at_start, old_start, new_start, contents);
        true
    }

    #[cfg(test)]
    pub(crate) fn with_contexts(files: Vec<DiffFile>, contexts: Vec<Option<String>>) -> Self {
        assert_eq!(files.len(), contexts.len());
        Self {
            files,
            contexts: contexts
                .into_iter()
                .map(|content| content.map(DiffFileContext::new))
                .collect(),
        }
    }
}

fn complete_added_or_deleted_source(file: &DiffFile, old: bool) -> Option<Arc<String>> {
    if (old && !file.is_deleted) || (!old && !file.is_new) {
        return None;
    }

    let mut expected_line = 1;
    let mut source = String::new();
    for hunk in &file.hunks {
        for line in &hunk.lines {
            let line_number = if old {
                line.old_number
            } else {
                line.new_number
            };
            let Some(line_number) = line_number else {
                continue;
            };
            if line_number != expected_line {
                return None;
            }
            source.push_str(hunk.line_content(line));
            source.push('\n');
            expected_line += 1;
        }
    }
    Some(Arc::new(source))
}

fn patched_source(file: &DiffFile, old: &DiffFileContext) -> Option<String> {
    let old_lines = old.lines(1, old.line_count as usize)?;
    let mut old_cursor = 1_u32;
    let mut source = String::with_capacity(old.content.len());

    for hunk in &file.hunks {
        // Git represents an insertion into an empty file as an old range beginning at line 0.
        let hunk_old_start = hunk.old_start.max(1);
        if hunk_old_start < old_cursor {
            return None;
        }
        append_old_lines(&mut source, &old_lines, old_cursor, hunk_old_start)?;
        old_cursor = hunk_old_start;

        for line in &hunk.lines {
            match line.kind {
                LineKind::Addition => {
                    source.push_str(hunk.line_content(line));
                    source.push('\n');
                }
                LineKind::Deletion => {
                    if line.old_number != Some(old_cursor) {
                        return None;
                    }
                    old_cursor += 1;
                }
                LineKind::Context => {
                    if line.old_number != Some(old_cursor) {
                        return None;
                    }
                    let content = old_lines.get(usize::try_from(old_cursor - 1).ok()?)?;
                    source.push_str(content);
                    source.push('\n');
                    old_cursor += 1;
                }
            }
        }
    }

    append_old_lines(
        &mut source,
        &old_lines,
        old_cursor,
        old.line_count.saturating_add(1),
    )?;
    Some(source)
}

fn append_old_lines(target: &mut String, old_lines: &[&str], start: u32, end: u32) -> Option<()> {
    let start = usize::try_from(start.checked_sub(1)?).ok()?;
    let end = usize::try_from(end.checked_sub(1)?).ok()?;
    for line in old_lines.get(start..end)? {
        target.push_str(line);
        target.push('\n');
    }
    Some(())
}

#[derive(Clone, Debug)]
struct DiffFileContext {
    content: Arc<String>,
    line_count: u32,
    line_ranges: OnceLock<Vec<Range<u32>>>,
}

impl DiffFileContext {
    fn new(content: String) -> Self {
        let line_count = u32::try_from(content.lines().count())
            .expect("diff source has more than u32::MAX lines");
        Self {
            content: Arc::new(content),
            line_count,
            line_ranges: OnceLock::new(),
        }
    }

    const fn line_count(&self) -> u32 {
        self.line_count
    }

    fn lines(&self, start: u32, count: usize) -> Option<Vec<&str>> {
        let ranges = self.line_ranges.get_or_init(|| self.build_line_ranges());
        let start = usize::try_from(start.checked_sub(1)?).ok()?;
        let ranges = ranges.get(start..start.checked_add(count)?)?;
        Some(
            ranges
                .iter()
                .map(|range| &self.content[range.start as usize..range.end as usize])
                .collect(),
        )
    }

    fn build_line_ranges(&self) -> Vec<Range<u32>> {
        let capacity =
            usize::try_from(self.line_count).expect("usize cannot hold source line count");
        let mut lines = Vec::with_capacity(capacity);
        let mut start = 0;
        for terminated_line in self.content.split_inclusive('\n') {
            let raw_line = terminated_line
                .strip_suffix('\n')
                .unwrap_or(terminated_line);
            let raw_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            let end = start + raw_line.len();
            lines.push(
                u32::try_from(start).expect("diff source exceeds 4 GiB")
                    ..u32::try_from(end).expect("diff source exceeds 4 GiB"),
            );
            start += terminated_line.len();
        }
        lines
    }
}

impl PartialEq for DiffFileContext {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
    }
}

impl Eq for DiffFileContext {}

pub fn parse_unified_diff(input: impl Into<String>) -> DiffSet {
    let input = Arc::new(input.into());
    let mut files = Vec::new();
    let mut current_file: Option<DiffFile> = None;
    let mut current_hunk: Option<SharedDiffHunkBuilder> = None;
    let mut old_number = 0;
    let mut new_number = 0;

    let finish_hunk = |file: &mut Option<DiffFile>, hunk: &mut Option<SharedDiffHunkBuilder>| {
        if let (Some(file), Some(hunk)) = (file.as_mut(), hunk.take()) {
            file.hunks.push(hunk.finish(input.clone()));
        }
    };

    let finish_file = |files: &mut Vec<DiffFile>, file: &mut Option<DiffFile>| {
        if let Some(file) = file.take() {
            files.push(file);
        }
    };

    let mut line_start = 0;
    for terminated_line in input.split_inclusive('\n') {
        let raw_line_start = line_start;
        line_start += terminated_line.len();
        let has_terminator = terminated_line.ends_with('\n');
        let raw_line = terminated_line
            .strip_suffix('\n')
            .unwrap_or(terminated_line);
        let raw_line = if has_terminator {
            raw_line.strip_suffix('\r').unwrap_or(raw_line)
        } else {
            raw_line
        };
        let content_end = raw_line_start + raw_line.len();

        if let Some(paths) = raw_line.strip_prefix("diff --git ") {
            finish_hunk(&mut current_file, &mut current_hunk);
            finish_file(&mut files, &mut current_file);
            let mut paths = paths.splitn(2, ' ');
            current_file = Some(DiffFile {
                old_path: paths.next().unwrap_or("a/unknown").to_owned(),
                new_path: paths.next().unwrap_or("b/unknown").to_owned(),
                hunks: Vec::new(),
                is_new: false,
                is_deleted: false,
            });
            continue;
        }

        if raw_line.starts_with("new file mode ") {
            if let Some(file) = current_file.as_mut() {
                file.is_new = true;
            }
            continue;
        }

        if raw_line.starts_with("deleted file mode ") {
            if let Some(file) = current_file.as_mut() {
                file.is_deleted = true;
            }
            continue;
        }

        if let Some(path) = raw_line.strip_prefix("--- ") {
            finish_hunk(&mut current_file, &mut current_hunk);
            if current_file.is_none() {
                current_file = Some(DiffFile {
                    old_path: path.to_owned(),
                    new_path: path.to_owned(),
                    hunks: Vec::new(),
                    is_new: path == "/dev/null",
                    is_deleted: false,
                });
            } else if let Some(file) = current_file.as_mut() {
                path.clone_into(&mut file.old_path);
                file.is_new = path == "/dev/null";
            }
            continue;
        }

        if let Some(path) = raw_line.strip_prefix("+++ ") {
            if let Some(file) = current_file.as_mut() {
                path.clone_into(&mut file.new_path);
                file.is_deleted = path == "/dev/null";
            }
            continue;
        }

        if raw_line.starts_with("@@") {
            finish_hunk(&mut current_file, &mut current_hunk);
            if current_file.is_none() {
                current_file = Some(DiffFile {
                    old_path: "a/unknown".into(),
                    new_path: "b/unknown".into(),
                    hunks: Vec::new(),
                    is_new: false,
                    is_deleted: false,
                });
            }
            let (parsed_old, parsed_new) = parse_hunk_starts(raw_line).unwrap_or((1, 1));
            old_number = parsed_old;
            new_number = parsed_new;
            current_hunk = Some(SharedDiffHunkBuilder::new(raw_line, old_number, new_number));
            continue;
        }

        let Some(hunk) = current_hunk.as_mut() else {
            continue;
        };

        if raw_line == "\\ No newline at end of file" {
            continue;
        }

        let kind = if raw_line.starts_with('+') {
            LineKind::Addition
        } else if raw_line.starts_with('-') {
            LineKind::Deletion
        } else if raw_line.starts_with(' ') {
            LineKind::Context
        } else {
            continue;
        };

        let (old_line_number, new_line_number) = match kind {
            LineKind::Context => {
                let numbers = (Some(old_number), Some(new_number));
                old_number += 1;
                new_number += 1;
                numbers
            }
            LineKind::Addition => {
                let numbers = (None, Some(new_number));
                new_number += 1;
                numbers
            }
            LineKind::Deletion => {
                let numbers = (Some(old_number), None);
                old_number += 1;
                numbers
            }
        };
        let content_start = u32::try_from(raw_line_start + 1).expect("diff input exceeds 4 GiB");
        let content_end = u32::try_from(content_end).expect("diff input exceeds 4 GiB");
        hunk.push_line(
            old_line_number,
            new_line_number,
            kind,
            content_start..content_end,
        );
    }

    finish_hunk(&mut current_file, &mut current_hunk);
    finish_file(&mut files, &mut current_file);
    DiffSet {
        files,
        contexts: Vec::new(),
    }
}

/// Convert libgit2's structured diff callbacks directly into the view model.
/// This path deliberately does not serialize and re-parse a unified patch.
pub fn from_git_diff(repository: &Repository, diff: &GitDiff<'_>) -> Result<DiffSet, git2::Error> {
    let mut files = Vec::with_capacity(diff.deltas().len());

    for delta in diff.deltas() {
        let status = delta.status();
        let raw_old_path = git_path(delta.old_file().path());
        let raw_new_path = git_path(delta.new_file().path());
        let old_path = if matches!(status, Delta::Added | Delta::Untracked) {
            "/dev/null".into()
        } else {
            raw_old_path.clone()
        };
        let new_path = if status == Delta::Deleted {
            "/dev/null".into()
        } else {
            raw_new_path.clone()
        };

        files.push(DiffFile {
            old_path,
            new_path,
            hunks: Vec::new(),
            is_new: matches!(status, Delta::Added | Delta::Untracked),
            is_deleted: status == Delta::Deleted,
        });
    }

    let hunk_builders = RefCell::new(
        (0..files.len())
            .map(|_| Vec::<DiffHunkBuilder>::new())
            .collect::<Vec<_>>(),
    );
    let next_file_index = Cell::new(0);
    let current_file_index = Cell::new(None);
    let file_count = files.len();
    // libgit2 invokes the file callback once per delta, before that file's hunk
    // and line callbacks, in the same order exposed by `diff.deltas()`.
    let mut file_callback = |_: git2::DiffDelta<'_>, _: f32| {
        let file_index = next_file_index.get();
        if file_index >= file_count {
            current_file_index.set(None);
            return false;
        }
        current_file_index.set(Some(file_index));
        next_file_index.set(file_index + 1);
        true
    };
    let mut hunk_callback = |_: git2::DiffDelta<'_>, hunk: git2::DiffHunk<'_>| {
        let Some(file_index) = current_file_index.get() else {
            return true;
        };
        hunk_builders.borrow_mut()[file_index].push(DiffHunkBuilder::new(
            String::from_utf8_lossy(hunk.header()).trim_end_matches(['\r', '\n']),
            hunk.old_start(),
            hunk.new_start(),
        ));
        true
    };
    let mut line_callback =
        |_: git2::DiffDelta<'_>, hunk: Option<git2::DiffHunk<'_>>, line: git2::DiffLine<'_>| {
            let kind = match line.origin_value() {
                DiffLineType::Context => LineKind::Context,
                DiffLineType::Addition => LineKind::Addition,
                DiffLineType::Deletion => LineKind::Deletion,
                _ => return true,
            };
            let Some(file_index) = current_file_index.get() else {
                return true;
            };
            let mut hunk_builders = hunk_builders.borrow_mut();
            let file_hunks = &mut hunk_builders[file_index];
            if file_hunks.is_empty()
                && let Some(hunk) = hunk
            {
                file_hunks.push(DiffHunkBuilder::new(
                    String::from_utf8_lossy(hunk.header()).trim_end_matches(['\r', '\n']),
                    hunk.old_start(),
                    hunk.new_start(),
                ));
            }
            if let Some(hunk) = file_hunks.last_mut() {
                hunk.push_line(
                    line.old_lineno(),
                    line.new_lineno(),
                    kind,
                    String::from_utf8_lossy(line.content()).trim_end_matches(['\r', '\n']),
                );
            }
            true
        };

    diff.foreach(
        &mut file_callback,
        None,
        Some(&mut hunk_callback),
        Some(&mut line_callback),
    )?;
    debug_assert_eq!(next_file_index.get(), file_count);
    let mut hunk_builders = hunk_builders.into_inner().into_iter();
    for file in &mut files {
        file.hunks = hunk_builders
            .next()
            .expect("hunk builder per diff file")
            .into_iter()
            .map(DiffHunkBuilder::finish)
            .collect();
    }
    let contexts = diff
        .deltas()
        .zip(&files)
        .map(|(delta, file)| {
            if file.hunks.is_empty() || file.is_new {
                return None;
            }
            repository
                .find_blob(delta.old_file().id())
                .ok()
                .and_then(|blob| std::str::from_utf8(blob.content()).ok().map(str::to_owned))
                .map(DiffFileContext::new)
        })
        .collect();
    Ok(DiffSet { files, contexts })
}

fn git_path(path: Option<&Path>) -> String {
    path.map_or_else(
        || "unknown".into(),
        |path| path.to_string_lossy().replace('\\', "/"),
    )
}

fn parse_hunk_starts(header: &str) -> Option<(u32, u32)> {
    let mut parts = header.split_whitespace();
    parts.next()?;
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    Some((parse_range_start(old)?, parse_range_start(new)?))
}

fn parse_range_start(range: &str) -> Option<u32> {
    range.split(',').next()?.parse().ok()
}

#[cfg(test)]
pub fn demo_diff() -> DiffSet {
    parse_unified_diff(DEMO_PATCH)
}

#[cfg(test)]
const DEMO_PATCH: &str = r#"diff --git a/src/auth/session.rs b/src/auth/session.rs
index c74db1a..29acdf2 100644
--- a/src/auth/session.rs
+++ b/src/auth/session.rs
@@ -18,10 +18,15 @@ pub struct Session {
     pub user_id: UserId,
     pub created_at: DateTime<Utc>,
-    pub expires_at: DateTime<Utc>,
+    pub expires_at: DateTime<Utc>,
+    pub last_seen_at: DateTime<Utc>,
+    pub device: Option<DeviceInfo>,
 }
 
-pub async fn create_session(user_id: UserId) -> Result<Session> {
-    let expires_at = Utc::now() + Duration::days(30);
+pub async fn create_session(
+    user_id: UserId,
+    device: Option<DeviceInfo>,
+    remember_me: bool,
+) -> Result<Session> {
+    let lifetime = if remember_me { 30 } else { 1 };
+    let expires_at = Utc::now() + Duration::days(lifetime);
     let session = Session {
@@ -29,7 +34,9 @@ pub async fn create_session(user_id: UserId) -> Result<Session> {
         user_id,
         created_at: Utc::now(),
         expires_at,
+        last_seen_at: Utc::now(),
+        device,
     };
 
-    database::sessions().insert(&session).await?;
+    database::sessions().insert(&session).await.context("persist session")?;
     Ok(session)
 }
diff --git a/src/components/avatar.tsx b/src/components/avatar.tsx
index 4c9a1bd..e8134de 100644
--- a/src/components/avatar.tsx
+++ b/src/components/avatar.tsx
@@ -8,12 +8,17 @@ type AvatarProps = {
   name: string;
   src?: string;
+  status?: "online" | "away" | "offline";
 };
 
-export function Avatar({ name, src }: AvatarProps) {
+export function Avatar({ name, src, status = "offline" }: AvatarProps) {
   return (
-    <div className="avatar">
+    <div className="avatar" data-status={status}>
       {src ? <img src={src} alt={name} /> : <Initials name={name} />}
+      <span className="avatar__status" aria-label={status} />
     </div>
   );
 }
diff --git a/README.md b/README.md
index a621984..d188ad4 100644
--- a/README.md
+++ b/README.md
@@ -2,6 +2,12 @@
 
 A fast desktop client for reviewing code changes.
 
+## Highlights
+
+- GPU-rendered interface powered by GPUI
+- Split and unified diff layouts
+- Inline syntax and change highlighting
+
 ## Development
 
-Run the app with `cargo run`.
+Run the demo with `cargo run`, or open a patch with `cargo run -- change.diff`.
"#;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{DiffFileContext, demo_diff, parse_unified_diff};

    #[test]
    fn parses_multiple_files_and_line_numbers() {
        let diff = demo_diff();
        assert_eq!(diff.files.len(), 3);
        assert_eq!(diff.files[0].display_path(), "src/auth/session.rs");
        assert_eq!(diff.files[0].hunks.len(), 2);
        assert_eq!(diff.files[0].hunks[0].lines[0].old_number, Some(18));
        assert_eq!(diff.files[0].hunks[0].lines[0].new_number, Some(18));
        assert!(diff.additions() > diff.deletions());
    }

    #[test]
    fn aligns_additions_and_deletions_by_kind() {
        let file = &demo_diff().files[1];
        assert_eq!(file.additions(), 4);
        assert_eq!(file.deletions(), 2);
    }

    #[test]
    fn handles_patch_without_diff_git_header() {
        let diff = parse_unified_diff("--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n");
        assert_eq!(diff.files.len(), 1);
        assert_eq!(diff.files[0].deletions(), 1);
        assert_eq!(diff.files[0].additions(), 1);
    }

    #[test]
    fn retains_files_with_metadata_only_changes() {
        let diff = parse_unified_diff(
            "diff --git a/script.sh b/script.sh\nold mode 100644\nnew mode 100755\n",
        );
        assert_eq!(diff.files.len(), 1);
        assert_eq!(diff.files[0].display_path(), "script.sh");
        assert!(diff.files[0].hunks.is_empty());
    }

    #[test]
    fn owned_patch_buffer_is_shared_without_copying_line_content() {
        let patch = String::from(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n@@ -3 +3 @@\n-before\n+after\n",
        );
        let patch_buffer = patch.as_ptr();
        let diff = parse_unified_diff(patch);
        let hunks = &diff.files[0].hunks;

        assert_eq!(hunks[0].content.as_ptr(), patch_buffer);
        assert!(Arc::ptr_eq(&hunks[0].content, &hunks[1].content));
        assert_eq!(hunks[0].line_content(&hunks[0].lines[0]), "old");
        assert_eq!(hunks[1].line_content(&hunks[1].lines[1]), "after");
    }

    #[test]
    fn shared_patch_ranges_strip_crlf_line_endings() {
        let diff =
            parse_unified_diff("--- a/a.txt\r\n+++ b/a.txt\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n");
        let hunk = &diff.files[0].hunks[0];

        assert_eq!(hunk.line_content(&hunk.lines[0]), "old");
        assert_eq!(hunk.line_content(&hunk.lines[1]), "new");
    }

    #[test]
    fn git_context_indexes_lines_lazily_and_strips_crlf() {
        let context = DiffFileContext::new("one\r\ntwo\r\nthree".into());

        assert_eq!(context.line_count(), 3);
        assert!(context.line_ranges.get().is_none());
        assert_eq!(context.lines(2, 2).unwrap(), ["two", "three"]);
        assert!(context.line_ranges.get().is_some());
    }

    #[test]
    fn syntax_source_reconstructs_each_requested_side_of_a_git_diff() {
        let mut diff = parse_unified_diff(
            "diff --git a/file.astro b/file.astro\n--- a/file.astro\n+++ b/file.astro\n@@ -2,3 +2,3 @@\n before\n-old\n+new\n after\n",
        );
        diff.contexts = vec![Some(DiffFileContext::new(
            "zero\nbefore\nold\nafter\ntail\n".into(),
        ))];

        let old = diff.syntax_source(0, true);
        let new = diff.syntax_source(0, false);

        assert_eq!(
            old.as_deref().map(String::as_str),
            Some("zero\nbefore\nold\nafter\ntail\n")
        );
        assert_eq!(
            new.as_deref().map(String::as_str),
            Some("zero\nbefore\nnew\nafter\ntail\n")
        );
    }

    #[test]
    fn syntax_source_recovers_a_deleted_files_complete_old_side() {
        let diff = parse_unified_diff(
            "diff --git a/file.astro b/file.astro\ndeleted file mode 100644\n--- a/file.astro\n+++ /dev/null\n@@ -1,2 +0,0 @@\n----\n-const title = \"Sabun\";\n",
        );

        let old = diff.syntax_source(0, true);
        let new = diff.syntax_source(0, false);

        assert_eq!(
            old.as_deref().map(String::as_str),
            Some("---\nconst title = \"Sabun\";\n")
        );
        assert_eq!(new.as_deref().map(String::as_str), Some(""));
    }
}
