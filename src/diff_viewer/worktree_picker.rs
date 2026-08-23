use std::path::PathBuf;

use super::{
    Context, DiffViewer, FluentBuilder, FontWeight, InteractiveElement, IntoElement, Palette,
    ParentElement, SOURCE_PICKER_SCROLLBAR_WIDTH, ScrollbarAxis, ScrollbarTarget,
    SourceCatalogLoadState, StatefulInteractiveElement, Styled, WorktreePickerState, div, point,
    px, relative, with_alpha,
};
use crate::cli::{GitDiffSourceSwitcher, GitWorktree};

impl DiffViewer {
    pub(super) fn toggle_worktree_picker(&mut self, cx: &mut Context<Self>) {
        if self.worktree_picker_state == WorktreePickerState::Open {
            self.close_worktree_picker(cx);
            return;
        }
        let has_multiple_worktrees = self
            .source_switcher
            .as_ref()
            .is_some_and(|switcher| switcher.worktrees().len() > 1);
        if !has_multiple_worktrees {
            return;
        }

        self.text_context_menu = None;
        self.path_context_menu = None;
        self.source_picker_open = false;
        self.source_catalog = None;
        self.source_catalog_load_state = SourceCatalogLoadState::Idle;
        self.source_catalog_generation = self.source_catalog_generation.wrapping_add(1);
        self.worktree_error = None;
        self.worktree_picker_scroll
            .set_offset(point(px(0.), px(0.)));
        self.worktree_picker_state = WorktreePickerState::Open;
        cx.notify();
    }

    fn close_worktree_picker(&mut self, cx: &mut Context<Self>) {
        if self.worktree_switching.is_some() {
            return;
        }
        self.worktree_picker_state = WorktreePickerState::Closed;
        self.worktree_error = None;
        cx.notify();
    }

    fn switch_worktree(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(source_switcher) = self.source_switcher.clone() else {
            return;
        };
        if self.worktree_switching.is_some() {
            return;
        }
        if source_switcher
            .current_worktree()
            .is_some_and(|worktree| worktree.path == path)
        {
            self.close_worktree_picker(cx);
            return;
        }

        self.worktree_switch_generation = self.worktree_switch_generation.wrapping_add(1);
        let generation = self.worktree_switch_generation;
        self.worktree_switching = Some(path.clone());
        self.worktree_error = None;
        cx.notify();

        cx.spawn(async move |viewer, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { source_switcher.switch_worktree(path) })
                .await;
            let _ = viewer.update(cx, |viewer, cx| {
                if viewer.worktree_switch_generation != generation {
                    return;
                }
                viewer.worktree_switching = None;
                match result {
                    Ok(input) => {
                        viewer.worktree_picker_state = WorktreePickerState::Closed;
                        viewer.worktree_error = None;
                        viewer.source_picker_open = false;
                        viewer.source_catalog = None;
                        viewer.source_catalog_load_state = SourceCatalogLoadState::Idle;
                        viewer.source_catalog_generation =
                            viewer.source_catalog_generation.wrapping_add(1);
                        viewer.viewed_files.clear();
                        viewer.collapsed_files.clear();
                        viewer.apply_input(input, cx);
                    }
                    Err(error) => {
                        viewer.worktree_error = Some(error);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn render_worktree_picker(
        &self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let worktrees = self
            .source_switcher
            .as_ref()
            .map(|switcher| switcher.worktrees().to_vec())
            .unwrap_or_default();
        let current_path = self
            .source_switcher
            .as_ref()
            .and_then(GitDiffSourceSwitcher::current_worktree)
            .map(|worktree| worktree.path);
        let switching = self.worktree_switching.clone();

        div()
            .id("worktree-picker-backdrop")
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .flex()
            .items_start()
            .justify_center()
            .bg(with_alpha(palette.canvas, 0.78))
            .occlude()
            .on_click(cx.listener(|this, _, _, cx| this.close_worktree_picker(cx)))
            .child(
                div()
                    .id("worktree-picker")
                    .mt(px(48.))
                    .w(relative(0.72))
                    .max_w(px(680.))
                    .max_h(relative(0.72))
                    .rounded_md()
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.elevated)
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                    .child(
                        div()
                            .h(px(64.))
                            .px_4()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(palette.border)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(palette.text)
                                            .child("Choose a worktree"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(palette.muted)
                                            .child("Switch the repository context for this view"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("close-worktree-picker")
                                    .size(px(32.))
                                    .rounded_md()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(17.))
                                    .text_color(palette.muted)
                                    .hover(|button| button.bg(palette.hover))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_worktree_picker(cx);
                                    }))
                                    .child("✕"),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .child(
                                div()
                                    .id("worktree-picker-scroll")
                                    .max_h(px(440.))
                                    .overflow_y_scroll()
                                    .track_scroll(&self.worktree_picker_scroll)
                                    .p_3()
                                    .pr(px(12. + SOURCE_PICKER_SCROLLBAR_WIDTH))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .children(worktrees.into_iter().enumerate().map(
                                        |(index, worktree)| {
                                            let active = current_path
                                                .as_ref()
                                                .is_some_and(|path| path == &worktree.path);
                                            let is_switching = switching
                                                .as_ref()
                                                .is_some_and(|path| path == &worktree.path);
                                            self.render_worktree_row(
                                                index,
                                                worktree,
                                                active,
                                                is_switching,
                                                palette,
                                                cx,
                                            )
                                        },
                                    ))
                                    .children(self.worktree_error.as_ref().map(|error| {
                                        div()
                                            .mt_2()
                                            .px_3()
                                            .py_2()
                                            .rounded_md()
                                            .bg(palette.red_bg)
                                            .text_xs()
                                            .text_color(palette.red)
                                            .child(error.clone())
                                    })),
                            )
                            .child(self.render_scrollbar(
                                ScrollbarTarget::WorktreePicker,
                                ScrollbarAxis::Vertical,
                                self.worktree_picker_scroll.clone(),
                                palette,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_worktree_row(
        &self,
        index: usize,
        worktree: GitWorktree,
        active: bool,
        switching: bool,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let path = worktree.path.clone();
        let path_label = worktree.path.to_string_lossy().into_owned();
        div()
            .id(("worktree-picker-row", index))
            .min_h(px(56.))
            .px_3()
            .py_2()
            .rounded_md()
            .flex()
            .items_center()
            .gap_3()
            .when(active, |row| row.bg(palette.selection))
            .hover(|row| row.bg(palette.hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.switch_worktree(path.clone(), cx);
            }))
            .child(
                div()
                    .w(px(12.))
                    .flex_none()
                    .text_color(palette.green)
                    .child(if active { "●" } else { "" }),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(palette.text)
                                    .child(worktree.name),
                            )
                            .children(switching.then(|| {
                                div().text_xs().text_color(palette.green).child("Loading…")
                            })),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_xs()
                            .text_color(palette.muted)
                            .child(format!("{} · {path_label}", worktree.branch)),
                    ),
            )
            .into_any_element()
    }
}
