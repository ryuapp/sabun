use std::ops::Range;

use super::{
    CONTEXT_LINES_PER_STEP, Context, ContextExpandDirection, ContextGap, ContextGapPosition,
    ContextGapSource, DiffLayout, DiffRowData, DiffViewer, FileTreeData, FileTreeRow,
    build_diff_row_data, build_file_tree_data, collapse_file_rows, normalized_path_components,
    point, px,
};

impl DiffViewer {
    pub(super) fn toggle_layout(&mut self, cx: &mut Context<Self>) {
        self.text_selection = None;
        self.header_text_selection = None;
        self.layout = match self.layout {
            DiffLayout::Split => DiffLayout::Unified,
            DiffLayout::Unified => DiffLayout::Split,
        };
        self.rebuild_diff_rows();
        cx.notify();
    }

    pub(super) fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.diff.files.len() {
            return;
        }
        self.selected_file = index;
        self.text_selection = None;
        self.header_text_selection = None;
        self.scroll_to_file(index);
        self.expand_file_ancestors(index);
        self.reveal_file_in_sidebar(index);
        cx.notify();
    }

    pub(super) fn toggle_directory(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.collapsed_directories.remove(&path) {
            self.collapsed_directories.insert(path);
        }
        self.rebuild_file_tree();
        self.file_smooth_scroll.reset();
        cx.notify();
    }

    pub(super) fn toggle_file_collapsed(
        &mut self,
        file_index: usize,
        sticky_header: bool,
        cx: &mut Context<Self>,
    ) {
        if file_index >= self.diff.files.len() {
            return;
        }
        let collapsing = !self.collapsed_files.remove(&file_index);
        if collapsing {
            self.collapsed_files.insert(file_index);
        }
        self.selected_file = file_index;
        self.text_selection = None;
        self.header_text_selection = None;
        self.text_context_menu = None;
        self.path_context_menu = None;
        self.cancel_diff_layout_zoom();

        let removed_rows = collapsing
            .then(|| {
                collapse_file_rows(
                    &mut self.diff_rows,
                    &mut self.diff_row_layouts,
                    &mut self.diff_row_offsets,
                    &mut self.file_row_indices,
                    file_index,
                )
            })
            .flatten();
        if let Some(removed_rows) = removed_rows {
            self.reindex_display_caches_after_removal(removed_rows);
            self.pending_scroll_file = None;
            if sticky_header {
                self.scroll_to_file(file_index);
            }
        } else {
            self.rebuild_diff_row_data();
            self.pending_scroll_file = sticky_header.then_some(file_index);
            self.clear_diff_caches();
        }
        self.diff_smooth_scroll.reset();
        cx.notify();
    }

    fn reindex_display_caches_after_removal(&mut self, removed_rows: Range<usize>) {
        let removed_count = removed_rows.len();
        self.syntax_cache = std::mem::take(&mut self.syntax_cache)
            .into_iter()
            .filter_map(|(mut key, value)| {
                if key.display_index >= removed_rows.end {
                    key.display_index -= removed_count;
                    Some((key, value))
                } else if key.display_index < removed_rows.start {
                    Some((key, value))
                } else {
                    None
                }
            })
            .collect();
        self.inline_cache = std::mem::take(&mut self.inline_cache)
            .into_iter()
            .filter_map(|(mut key, value)| {
                if key.display_index >= removed_rows.end {
                    key.display_index -= removed_count;
                    Some((key, value))
                } else if key.display_index < removed_rows.start {
                    Some((key, value))
                } else {
                    None
                }
            })
            .collect();
    }

    pub(super) fn rebuild_file_tree(&mut self) {
        let FileTreeData { rows, offsets } =
            build_file_tree_data(&self.diff.files, &self.collapsed_directories);
        self.file_tree_rows = rows;
        self.file_tree_row_offsets = offsets;
    }

    pub(super) fn set_layout(&mut self, layout: DiffLayout, cx: &mut Context<Self>) {
        if self.layout == layout {
            return;
        }
        self.text_selection = None;
        self.header_text_selection = None;
        self.layout = layout;
        self.rebuild_diff_rows();
        cx.notify();
    }

    fn rebuild_diff_rows(&mut self) {
        self.cancel_diff_layout_zoom();
        self.rebuild_diff_row_data();
        self.diff_scroll.set_offset(point(px(0.), px(0.)));
        self.diff_smooth_scroll.reset();
        self.pending_scroll_file = Some(self.selected_file);
        self.clear_diff_caches();
    }

    pub(super) fn rebuild_diff_row_data(&mut self) {
        let DiffRowData {
            rows,
            layouts,
            file_row_indices,
            offsets,
            context_gaps,
        } = build_diff_row_data(
            &self.diff,
            self.layout,
            &self.collapsed_files,
            &self.context_expansions,
        );
        self.diff_rows = rows;
        self.diff_row_layouts = layouts;
        self.file_row_indices = file_row_indices;
        self.diff_row_context_gaps = context_gaps;
        self.diff_row_offsets = offsets;
        self.wrapped_offsets_range = None;
    }

    fn clear_diff_caches(&mut self) {
        self.syntax_cache.clear();
        self.syntax_streams.clear();
        self.inline_cache.clear();
    }

    pub(super) fn expand_context(
        &mut self,
        gap: ContextGap,
        direction: ContextExpandDirection,
        cx: &mut Context<Self>,
    ) {
        if let ContextGapSource::File {
            old_start,
            new_start,
        } = gap.source
        {
            let reveal = (gap.end - gap.start).min(CONTEXT_LINES_PER_STEP);
            let (hunk_index, at_start, offset) = match (gap.position, direction) {
                (ContextGapPosition::Leading, ContextExpandDirection::Up) => {
                    (gap.hunk_index, true, gap.end - reveal)
                }
                (ContextGapPosition::Middle, ContextExpandDirection::Up) => {
                    (gap.hunk_index + 1, true, gap.end - reveal)
                }
                (
                    ContextGapPosition::Middle | ContextGapPosition::Trailing,
                    ContextExpandDirection::Down,
                ) => (gap.hunk_index, false, 0),
                _ => return,
            };
            let Ok(offset) = u32::try_from(offset) else {
                return;
            };
            if !self.diff.insert_context(
                gap.file_index,
                hunk_index,
                at_start,
                old_start.saturating_add(offset),
                new_start.saturating_add(offset),
                reveal,
            ) {
                return;
            }
            self.finish_context_expansion(cx);
            return;
        }

        let expansion = self.context_expansions.entry(gap).or_default();
        if !expansion.reveal(gap, direction) {
            return;
        }
        self.finish_context_expansion(cx);
    }

    fn finish_context_expansion(&mut self, cx: &mut Context<Self>) {
        self.cancel_diff_layout_zoom();
        let offset = self.diff_scroll.offset();
        self.rebuild_diff_row_data();
        self.diff_scroll.set_offset(offset);
        self.diff_smooth_scroll.stop_at(offset);
        self.clear_diff_caches();
        cx.notify();
    }

    pub(super) fn scroll_to_file(&mut self, file_index: usize) {
        let Some(&row_index) = self.file_row_indices.get(file_index) else {
            return;
        };
        let offsets = self.diff_offsets();
        let Some(top) = offsets.get(row_index) else {
            return;
        };
        let current = self.diff_scroll.offset();
        let content_height = offsets.last().unwrap_or_default();
        let viewport_height = self.diff_scroll.bounds().size.height;
        let max_y = (content_height - viewport_height).max(px(0.));
        let target = point(current.x, (-top).clamp(-max_y, px(0.)));
        self.diff_scroll.set_offset(target);
        self.diff_smooth_scroll.stop_at(target);
    }

    fn reveal_file_in_sidebar(&mut self, file_index: usize) {
        let viewport = self.file_scroll.bounds().size.height;
        if viewport <= px(0.) {
            return;
        }
        let Some(row_index) = self.file_tree_rows.iter().position(
            |row| matches!(row, FileTreeRow::File { file_index: index, .. } if *index == file_index),
        ) else {
            return;
        };
        let row_top = self.file_tree_row_offsets[row_index];
        let row_bottom = self.file_tree_row_offsets[row_index + 1];
        let current = self.file_scroll.offset();
        let visible_top = -current.y;
        let visible_bottom = visible_top + viewport;
        let next_y = if row_top < visible_top {
            -row_top
        } else if row_bottom > visible_bottom {
            -(row_bottom - viewport)
        } else {
            return;
        };
        let content_height = self
            .file_tree_row_offsets
            .last()
            .copied()
            .unwrap_or_default();
        let max_y = (content_height - viewport).max(px(0.));
        let target = point(current.x, next_y.clamp(-max_y, px(0.)));
        self.file_scroll.set_offset(target);
        self.file_smooth_scroll.stop_at(target);
    }

    fn expand_file_ancestors(&mut self, file_index: usize) {
        let Some(file) = self.diff.files.get(file_index) else {
            return;
        };
        let mut path = String::new();
        let components = normalized_path_components(file.display_path());
        let mut changed = false;
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(component);
            changed |= self.collapsed_directories.remove(&path);
        }
        if changed {
            self.rebuild_file_tree();
        }
    }

    pub(super) fn sync_selected_file_from_scroll(&mut self) {
        if self.diff.files.is_empty() || self.diff_rows.is_empty() {
            return;
        }
        let offset = self.diff_scroll.offset();
        let max_offset = self.diff_scroll.max_offset();
        let at_bottom = max_offset.height > px(0.) && offset.y <= -max_offset.height + px(1.);
        if at_bottom {
            let file_index = self.diff.files.len() - 1;
            if self.selected_file != file_index {
                self.selected_file = file_index;
                self.reveal_file_in_sidebar(file_index);
            }
            return;
        }

        let visible_top = (-offset.y).max(px(0.));
        if self.collapsed_files.contains(&self.selected_file)
            && let Some(header_top) = self
                .file_row_indices
                .get(self.selected_file)
                .and_then(|row_index| self.diff_offsets().get(*row_index))
            && visible_top >= header_top - px(1.)
            && visible_top <= header_top + px(1.)
        {
            return;
        }

        let marker = (self.diff_scroll.bounds().size.height * 0.18).min(px(96.));
        let position = (-offset.y + marker).max(px(0.));
        let row_index = self
            .diff_offsets()
            .row_index_at(position, self.diff_rows.len());
        if let Some(file_index) = self.diff_rows.file_index_at(row_index)
            && self.selected_file != file_index
        {
            self.selected_file = file_index;
            self.reveal_file_in_sidebar(file_index);
        }
    }

    pub(super) fn sticky_file_index(&self) -> Option<usize> {
        if self.diff_rows.is_empty() {
            return None;
        }
        let position = (-self.diff_scroll.offset().y).max(px(0.));
        let row_index = self
            .diff_offsets()
            .row_index_at(position, self.diff_rows.len());
        let file_index = self
            .diff_rows
            .file_index_at(row_index)
            .or_else(|| self.diff_rows.iter().find_map(|row| row.file_index()))?;
        let header_top = self
            .file_row_indices
            .get(file_index)
            .and_then(|row_index| self.diff_offsets().get(*row_index))?;

        (position > header_top).then_some(file_index)
    }

    pub(super) fn next_file(&mut self, cx: &mut Context<Self>) {
        if !self.diff.files.is_empty() {
            self.select_file((self.selected_file + 1) % self.diff.files.len(), cx);
        }
    }

    pub(super) fn previous_file(&mut self, cx: &mut Context<Self>) {
        if !self.diff.files.is_empty() {
            let index = if self.selected_file == 0 {
                self.diff.files.len() - 1
            } else {
                self.selected_file - 1
            };
            self.select_file(index, cx);
        }
    }
}
