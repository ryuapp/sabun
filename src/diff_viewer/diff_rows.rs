use super::{
    ContextExpansion, ContextGap, ContextGapPosition, ContextGapSource, DiffDisplayRow, DiffFile,
    DiffHunk, DiffLayout, DiffLine, DiffRowLayout, DiffSet, HashMap, HashSet, LineKind,
    NO_DIFF_ROW_INDEX, Pixels, Range, diff_row_layouts, pack_diff_row_index,
};

#[derive(Clone, Debug)]
enum DiffRowChunk {
    Single(DiffDisplayRow),
    Split {
        file_index: u32,
        hunk_index: u32,
        old_start: u32,
        new_start: u32,
        len: u32,
    },
    Unified {
        file_index: u32,
        hunk_index: u32,
        row_start: u32,
        len: u32,
        counterpart_start: u32,
    },
}

impl DiffRowChunk {
    const fn len(&self) -> usize {
        match self {
            Self::Single(_) => 1,
            Self::Split { len, .. } | Self::Unified { len, .. } => *len as usize,
        }
    }

    fn row(&self, offset: usize) -> Option<DiffDisplayRow> {
        match self {
            Self::Single(row) => (offset == 0).then_some(*row),
            Self::Split {
                file_index,
                hunk_index,
                old_start,
                new_start,
                len,
            } => {
                let offset = u32::try_from(offset).ok()?;
                (offset < *len).then(|| DiffDisplayRow::Split {
                    file_index: *file_index,
                    hunk_index: *hunk_index,
                    old_line_index: if *old_start == NO_DIFF_ROW_INDEX {
                        NO_DIFF_ROW_INDEX
                    } else {
                        old_start + offset
                    },
                    new_line_index: if *new_start == NO_DIFF_ROW_INDEX {
                        NO_DIFF_ROW_INDEX
                    } else {
                        new_start + offset
                    },
                })
            }
            Self::Unified {
                file_index,
                hunk_index,
                row_start,
                len,
                counterpart_start,
            } => {
                let offset = u32::try_from(offset).ok()?;
                (offset < *len).then(|| DiffDisplayRow::Unified {
                    file_index: *file_index,
                    hunk_index: *hunk_index,
                    row_index: row_start + offset,
                    counterpart_index: if *counterpart_start == NO_DIFF_ROW_INDEX {
                        NO_DIFF_ROW_INDEX
                    } else {
                        counterpart_start + offset
                    },
                })
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct DiffRowIndex {
    chunks: Vec<DiffRowChunk>,
    chunk_starts: Vec<usize>,
    len: usize,
}

impl DiffRowIndex {
    pub(super) const fn len(&self) -> usize {
        self.len
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[cfg(test)]
    pub(super) const fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub(super) fn get(&self, index: usize) -> Option<DiffDisplayRow> {
        if index >= self.len {
            return None;
        }
        let chunk_index = self
            .chunk_starts
            .partition_point(|start| *start <= index)
            .saturating_sub(1);
        self.chunks
            .get(chunk_index)?
            .row(index - self.chunk_starts[chunk_index])
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = DiffDisplayRow> + '_ {
        self.chunks
            .iter()
            .flat_map(|chunk| (0..chunk.len()).filter_map(|offset| chunk.row(offset)))
    }

    pub(super) fn file_index_at(&self, mut index: usize) -> Option<usize> {
        loop {
            if let Some(file_index) = self.get(index).and_then(|row| row.file_index()) {
                return Some(file_index);
            }
            index = index.checked_sub(1)?;
        }
    }

    fn push_chunk(&mut self, chunk: DiffRowChunk) {
        let chunk_len = chunk.len();
        if chunk_len == 0 {
            return;
        }
        self.chunk_starts.push(self.len);
        self.chunks.push(chunk);
        self.len += chunk_len;
    }

    fn push_single(&mut self, row: DiffDisplayRow) {
        self.push_chunk(DiffRowChunk::Single(row));
    }

    fn push_split(
        &mut self,
        file_index: usize,
        hunk_index: usize,
        rows: &[(Option<usize>, Option<usize>)],
    ) {
        let mut start = 0;
        while start < rows.len() {
            let (old_start, new_start) = rows[start];
            let mut end = start + 1;
            while end < rows.len()
                && rows[end].0 == old_start.map(|line| line + end - start)
                && rows[end].1 == new_start.map(|line| line + end - start)
            {
                end += 1;
            }
            self.push_chunk(DiffRowChunk::Split {
                file_index: pack_diff_row_index(file_index),
                hunk_index: pack_diff_row_index(hunk_index),
                old_start: old_start.map_or(NO_DIFF_ROW_INDEX, pack_diff_row_index),
                new_start: new_start.map_or(NO_DIFF_ROW_INDEX, pack_diff_row_index),
                len: pack_diff_row_index(end - start),
            });
            start = end;
        }
    }

    fn push_unified(
        &mut self,
        file_index: usize,
        hunk_index: usize,
        range: Range<usize>,
        counterparts: &[Option<usize>],
    ) {
        let mut start = range.start;
        while start < range.end {
            let counterpart_start = counterparts[start];
            let mut end = start + 1;
            while end < range.end
                && counterparts[end]
                    == counterpart_start.map(|counterpart| counterpart + end - start)
            {
                end += 1;
            }
            self.push_chunk(DiffRowChunk::Unified {
                file_index: pack_diff_row_index(file_index),
                hunk_index: pack_diff_row_index(hunk_index),
                row_start: pack_diff_row_index(start),
                len: pack_diff_row_index(end - start),
                counterpart_start: counterpart_start.map_or(NO_DIFF_ROW_INDEX, pack_diff_row_index),
            });
            start = end;
        }
    }

    fn extend(&mut self, other: Self) {
        for chunk in other.chunks {
            self.push_chunk(chunk);
        }
    }

    fn drain(&mut self, range: Range<usize>) -> bool {
        let start_chunk = self
            .chunk_starts
            .partition_point(|start| *start < range.start);
        let end_chunk = self
            .chunk_starts
            .partition_point(|start| *start < range.end);
        if self.chunk_starts.get(start_chunk).copied() != Some(range.start)
            || self
                .chunk_starts
                .get(end_chunk)
                .copied()
                .unwrap_or(self.len)
                != range.end
        {
            return false;
        }
        self.chunks.drain(start_chunk..end_chunk);
        self.chunk_starts.drain(start_chunk..end_chunk);
        let removed = range.len();
        for start in &mut self.chunk_starts[start_chunk..] {
            *start -= removed;
        }
        self.len -= removed;
        true
    }
}

fn align_line_indices(
    lines: &[DiffLine],
    range: Range<usize>,
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut rows = Vec::new();
    let mut index = range.start;
    while index < range.end {
        if lines[index].kind == LineKind::Context {
            rows.push((Some(index), Some(index)));
            index += 1;
            continue;
        }

        let start = index;
        while index < range.end && lines[index].kind != LineKind::Context {
            index += 1;
        }
        let mut deletions =
            (start..index).filter(|line_index| lines[*line_index].kind == LineKind::Deletion);
        let mut additions =
            (start..index).filter(|line_index| lines[*line_index].kind == LineKind::Addition);
        loop {
            let deletion = deletions.next();
            let addition = additions.next();
            if deletion.is_none() && addition.is_none() {
                break;
            }
            rows.push((deletion, addition));
        }
    }
    rows
}

pub(super) struct DiffRowData {
    pub(super) rows: DiffRowIndex,
    pub(super) layouts: Vec<DiffRowLayout>,
    pub(super) file_row_indices: Vec<usize>,
    pub(super) offsets: Vec<Pixels>,
    pub(super) context_gaps: Vec<ContextGap>,
}

pub(super) fn build_diff_row_data(
    diff: &DiffSet,
    layout: DiffLayout,
    collapsed_files: &HashSet<usize>,
    context_expansions: &HashMap<ContextGap, ContextExpansion>,
) -> DiffRowData {
    let (rows, file_row_indices, context_gaps) = build_diff_rows_with_context(
        &diff.files,
        layout,
        collapsed_files,
        context_expansions,
        |file_index| diff.old_line_count(file_index),
    );
    let offsets = row_offsets(&rows);
    let layouts = diff_row_layouts(&rows, &diff.files);
    DiffRowData {
        rows,
        layouts,
        file_row_indices,
        offsets,
        context_gaps,
    }
}

#[cfg(test)]
pub(super) fn build_diff_rows(
    files: &[DiffFile],
    layout: DiffLayout,
    collapsed_files: &HashSet<usize>,
    context_expansions: &HashMap<ContextGap, ContextExpansion>,
) -> (DiffRowIndex, Vec<usize>, Vec<ContextGap>) {
    build_diff_rows_with_context(files, layout, collapsed_files, context_expansions, |_| None)
}

fn build_diff_rows_with_context(
    files: &[DiffFile],
    layout: DiffLayout,
    collapsed_files: &HashSet<usize>,
    context_expansions: &HashMap<ContextGap, ContextExpansion>,
    old_line_count: impl Fn(usize) -> Option<u32>,
) -> (DiffRowIndex, Vec<usize>, Vec<ContextGap>) {
    let mut rows = DiffRowIndex::default();
    let mut context_gaps = Vec::new();
    let mut file_row_indices = Vec::with_capacity(files.len());
    for (file_index, file) in files.iter().enumerate() {
        if file_index > 0 && !collapsed_files.contains(&(file_index - 1)) {
            rows.push_single(DiffDisplayRow::FileGap);
        }
        file_row_indices.push(rows.len());
        rows.push_single(DiffDisplayRow::FileHeader {
            file_index: pack_diff_row_index(file_index),
        });
        if !collapsed_files.contains(&file_index) {
            rows.extend(build_file_diff_rows_with_context(
                file,
                file_index,
                layout,
                context_expansions,
                &mut context_gaps,
                old_line_count(file_index),
            ));
        }
    }
    (rows, file_row_indices, context_gaps)
}

pub(super) fn collapse_file_rows(
    rows: &mut DiffRowIndex,
    layouts: &mut Vec<DiffRowLayout>,
    offsets: &mut Vec<Pixels>,
    file_row_indices: &mut [usize],
    file_index: usize,
) -> Option<Range<usize>> {
    if layouts.len() != rows.len() || offsets.len() != rows.len() + 1 {
        return None;
    }
    let header_index = *file_row_indices.get(file_index)?;
    let start = header_index.checked_add(1)?;
    let end = file_row_indices
        .get(file_index + 1)
        .copied()
        .unwrap_or(rows.len());
    if start >= end || end > rows.len() {
        return None;
    }

    let removed_height = offsets[end] - offsets[start];
    let removed_count = end - start;
    if !rows.drain(start..end) {
        return None;
    }
    layouts.drain(start..end);
    offsets.drain(start + 1..=end);
    for offset in &mut offsets[start + 1..] {
        *offset -= removed_height;
    }
    for row_index in &mut file_row_indices[file_index + 1..] {
        *row_index -= removed_count;
    }

    Some(start..end)
}

#[cfg(test)]
pub(super) fn build_file_diff_rows(
    file: &DiffFile,
    file_index: usize,
    layout: DiffLayout,
    context_expansions: &HashMap<ContextGap, ContextExpansion>,
    context_gaps: &mut Vec<ContextGap>,
) -> DiffRowIndex {
    build_file_diff_rows_with_context(
        file,
        file_index,
        layout,
        context_expansions,
        context_gaps,
        None,
    )
}

fn build_file_diff_rows_with_context(
    file: &DiffFile,
    file_index: usize,
    layout: DiffLayout,
    context_expansions: &HashMap<ContextGap, ContextExpansion>,
    context_gaps: &mut Vec<ContextGap>,
    old_line_count: Option<u32>,
) -> DiffRowIndex {
    let mut rows = DiffRowIndex::default();
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        if hunk_index == 0
            && let Some(old_line_count) = old_line_count
            && hunk.old_start > 1
        {
            let hidden = hunk.old_start - 1;
            push_file_context_gap(
                &mut rows,
                context_gaps,
                ContextGap {
                    file_index,
                    hunk_index,
                    start: 0,
                    end: hidden as usize,
                    position: ContextGapPosition::Leading,
                    source: ContextGapSource::File {
                        old_start: 1,
                        new_start: hunk.new_start.saturating_sub(hidden),
                    },
                },
            );
            debug_assert!(old_line_count >= hidden);
        } else if hunk_index > 0 {
            let previous = &file.hunks[hunk_index - 1];
            let old_start = hunk_end(previous, true).saturating_add(1);
            let new_start = hunk_end(previous, false).saturating_add(1);
            let hidden = hunk.old_start.saturating_sub(old_start);
            if old_line_count.is_some() {
                debug_assert_eq!(hidden, hunk.new_start.saturating_sub(new_start));
                push_file_context_gap(
                    &mut rows,
                    context_gaps,
                    ContextGap {
                        file_index,
                        hunk_index: hunk_index - 1,
                        start: 0,
                        end: hidden as usize,
                        position: ContextGapPosition::Middle,
                        source: ContextGapSource::File {
                            old_start,
                            new_start,
                        },
                    },
                );
            } else if hidden > 0 {
                rows.push_single(DiffDisplayRow::Separator {
                    file_index: pack_diff_row_index(file_index),
                    hidden,
                    gap_index: NO_DIFF_ROW_INDEX,
                    position: None,
                });
            }
        }
        let counterparts = (layout == DiffLayout::Unified).then(|| paired_line_indices(hunk));
        let segments = if old_line_count.is_some() {
            vec![HunkSegment::Lines(0..hunk.lines.len())]
        } else {
            hunk_segments(file_index, hunk_index, hunk, context_expansions)
        };
        for segment in segments {
            match segment {
                HunkSegment::Lines(range) => match layout {
                    DiffLayout::Split => {
                        let split_rows = align_line_indices(&hunk.lines, range.clone());
                        rows.push_split(file_index, hunk_index, &split_rows);
                    }
                    DiffLayout::Unified => {
                        let counterparts = counterparts.as_ref().expect("unified counterparts");
                        rows.push_unified(file_index, hunk_index, range, counterparts);
                    }
                },
                HunkSegment::Gap { gap, hidden } => {
                    let gap_index = pack_diff_row_index(context_gaps.len());
                    context_gaps.push(gap);
                    rows.push_single(DiffDisplayRow::Separator {
                        file_index: pack_diff_row_index(file_index),
                        hidden: pack_diff_row_index(hidden),
                        gap_index,
                        position: Some(gap.position),
                    });
                }
            }
        }
    }

    if let Some((old_line_count, (hunk_index, hunk))) =
        old_line_count.zip(file.hunks.iter().enumerate().next_back())
    {
        let old_start = hunk_end(hunk, true).saturating_add(1);
        let hidden = old_line_count.saturating_sub(old_start.saturating_sub(1));
        push_file_context_gap(
            &mut rows,
            context_gaps,
            ContextGap {
                file_index,
                hunk_index,
                start: 0,
                end: hidden as usize,
                position: ContextGapPosition::Trailing,
                source: ContextGapSource::File {
                    old_start,
                    new_start: hunk_end(hunk, false).saturating_add(1),
                },
            },
        );
    }

    if rows.is_empty() {
        rows.push_single(DiffDisplayRow::Empty {
            file_index: pack_diff_row_index(file_index),
        });
    }
    rows
}

fn hunk_end(hunk: &DiffHunk, old: bool) -> u32 {
    hunk.lines
        .iter()
        .filter_map(|line| {
            if old {
                line.old_number
            } else {
                line.new_number
            }
        })
        .max()
        .unwrap_or(if old { hunk.old_start } else { hunk.new_start })
}

fn push_file_context_gap(
    rows: &mut DiffRowIndex,
    context_gaps: &mut Vec<ContextGap>,
    gap: ContextGap,
) {
    let hidden = gap.end.saturating_sub(gap.start);
    if hidden == 0 {
        return;
    }
    let gap_index = pack_diff_row_index(context_gaps.len());
    context_gaps.push(gap);
    rows.push_single(DiffDisplayRow::Separator {
        file_index: pack_diff_row_index(gap.file_index),
        hidden: pack_diff_row_index(hidden),
        gap_index,
        position: Some(gap.position),
    });
}

const VISIBLE_CONTEXT_LINES: usize = 3;

enum HunkSegment {
    Lines(Range<usize>),
    Gap { gap: ContextGap, hidden: usize },
}

fn hunk_segments(
    file_index: usize,
    hunk_index: usize,
    hunk: &DiffHunk,
    context_expansions: &HashMap<ContextGap, ContextExpansion>,
) -> Vec<HunkSegment> {
    let mut segments = Vec::new();
    let mut visible_start = 0;
    let mut index = 0;
    while index < hunk.lines.len() {
        if hunk.lines[index].kind != LineKind::Context {
            index += 1;
            continue;
        }
        let context_start = index;
        while index < hunk.lines.len() && hunk.lines[index].kind == LineKind::Context {
            index += 1;
        }
        let context_end = index;
        if context_start == 0 && context_end == hunk.lines.len() {
            continue;
        }
        let visible_context = if context_start == 0 || context_end == hunk.lines.len() {
            VISIBLE_CONTEXT_LINES
        } else {
            VISIBLE_CONTEXT_LINES * 2
        };
        if context_end - context_start <= visible_context {
            continue;
        }
        let gap = ContextGap {
            file_index,
            hunk_index,
            start: if context_start == 0 {
                context_start
            } else {
                context_start + VISIBLE_CONTEXT_LINES
            },
            end: if context_end == hunk.lines.len() {
                context_end
            } else {
                context_end - VISIBLE_CONTEXT_LINES
            },
            position: if context_start == 0 {
                ContextGapPosition::Leading
            } else if context_end == hunk.lines.len() {
                ContextGapPosition::Trailing
            } else {
                ContextGapPosition::Middle
            },
            source: ContextGapSource::Hunk,
        };
        let expansion = context_expansions.get(&gap).copied().unwrap_or_default();
        let remaining_start = gap.start.saturating_add(expansion.from_start).min(gap.end);
        let remaining_end = gap
            .end
            .saturating_sub(expansion.from_end)
            .max(remaining_start);
        if remaining_start == remaining_end {
            continue;
        }
        if visible_start < remaining_start {
            segments.push(HunkSegment::Lines(visible_start..remaining_start));
        }
        segments.push(HunkSegment::Gap {
            gap,
            hidden: remaining_end - remaining_start,
        });
        visible_start = remaining_end;
    }
    if visible_start < hunk.lines.len() {
        segments.push(HunkSegment::Lines(visible_start..hunk.lines.len()));
    }
    segments
}

fn paired_line_indices(hunk: &DiffHunk) -> Vec<Option<usize>> {
    let mut counterparts = vec![None; hunk.lines.len()];
    let mut block_start = 0;
    while block_start < hunk.lines.len() {
        if hunk.lines[block_start].kind == LineKind::Context {
            block_start += 1;
            continue;
        }
        let mut block_end = block_start;
        while block_end < hunk.lines.len() && hunk.lines[block_end].kind != LineKind::Context {
            block_end += 1;
        }
        let deletion_indexes =
            (block_start..block_end).filter(|index| hunk.lines[*index].kind == LineKind::Deletion);
        let addition_indexes =
            (block_start..block_end).filter(|index| hunk.lines[*index].kind == LineKind::Addition);
        for (deletion, addition) in deletion_indexes.zip(addition_indexes) {
            counterparts[deletion] = Some(addition);
            counterparts[addition] = Some(deletion);
        }
        block_start = block_end;
    }
    counterparts
}

pub(super) fn row_offsets(rows: &DiffRowIndex) -> Vec<Pixels> {
    let mut offsets = Vec::with_capacity(rows.len() + 1);
    let mut total = super::px(0.);
    offsets.push(total);
    for row in rows.iter() {
        total += row.height();
        offsets.push(total);
    }
    offsets
}

#[cfg(test)]
pub(super) fn paired_line(hunk: &DiffHunk, index: usize) -> Option<&DiffLine> {
    let line = hunk.lines.get(index)?;
    let block_start = hunk.lines[..index]
        .iter()
        .rposition(|candidate| candidate.kind == LineKind::Context)
        .map_or(0, |context_index| context_index + 1);
    let block_end = hunk.lines[index + 1..]
        .iter()
        .position(|candidate| candidate.kind == LineKind::Context)
        .map_or(hunk.lines.len(), |offset| index + 1 + offset);
    let block = &hunk.lines[block_start..block_end];
    let index_in_block = index - block_start;

    match line.kind {
        LineKind::Deletion => block
            .iter()
            .filter(|candidate| candidate.kind == LineKind::Addition)
            .nth(
                block[..index_in_block]
                    .iter()
                    .filter(|candidate| candidate.kind == LineKind::Deletion)
                    .count(),
            ),
        LineKind::Addition => block
            .iter()
            .filter(|candidate| candidate.kind == LineKind::Deletion)
            .nth(
                block[..index_in_block]
                    .iter()
                    .filter(|candidate| candidate.kind == LineKind::Addition)
                    .count(),
            ),
        LineKind::Context => None,
    }
}
