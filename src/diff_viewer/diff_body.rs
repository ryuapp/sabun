use super::{
    Context, ContextExpandDirection, ContextGap, ContextGapPosition, ContextSeparator,
    DEFAULT_DIFF_FONT_SIZE, DiffDisplayRow, DiffLayout, DiffViewer, EmptyState,
    ExpandIconDirection, FluentBuilder, InteractiveElement, IntoElement, MouseButton, Palette,
    ParentElement, SCROLLBAR_WIDTH, ScrollViewport, ScrollbarAxis, ScrollbarTarget,
    SmoothScrollTarget, StatefulInteractiveElement, Styled, VirtualizedColumn, context_expand_icon,
    diff_gutter_width, div, px, unpack_diff_row_index,
};

impl DiffViewer {
    pub(super) fn render_diff_body(
        &mut self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let sticky_file_header = self.sticky_file_header();
        let (visible_rows, top_space, bottom_space) =
            self.diff_offsets().visible_range(&self.diff_scroll, 108.);
        let mut rows = Vec::with_capacity(visible_rows.len());
        for display_index in visible_rows {
            rows.push(self.render_display_row(display_index, palette, cx));
        }

        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .relative()
            .overflow_hidden()
            .bg(palette.canvas)
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event, window, cx| {
                    this.toggle_middle_auto_scroll(SmoothScrollTarget::Diff, event, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event, _, cx| this.open_text_context_menu(event, cx)),
            )
            .child(ScrollViewport::new(
                "diff-scroll",
                self.diff_scroll.clone(),
                px(SCROLLBAR_WIDTH),
                div()
                    .w_full()
                    .flex_none()
                    .font_family("Cascadia Mono")
                    .text_size(self.diff_font_size)
                    .child(VirtualizedColumn::new(rows, top_space, bottom_space)),
                cx.listener(|this, event, window, cx| {
                    this.scroll_wheel(SmoothScrollTarget::Diff, event, window, cx);
                }),
            ))
            .when_some(sticky_file_header, |container, (file_index, top)| {
                container.child(
                    div()
                        .absolute()
                        .top(top)
                        .left_0()
                        .right_0()
                        .child(self.render_file_header(file_index, palette, true, cx))
                        .on_scroll_wheel(cx.listener(|this, event, window, cx| {
                            this.scroll_wheel(SmoothScrollTarget::Diff, event, window, cx);
                        })),
                )
            })
            .child(self.render_scrollbar(
                ScrollbarTarget::DiffVertical,
                ScrollbarAxis::Vertical,
                self.diff_scroll.clone(),
                palette,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_display_row(
        &mut self,
        display_index: usize,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(row) = self.diff_rows.get(display_index) else {
            return div().into_any_element();
        };
        match row {
            DiffDisplayRow::FileGap => div()
                .h(px(18.))
                .flex_none()
                .border_y_1()
                .border_color(palette.border)
                .bg(palette.sidebar)
                .into_any_element(),
            DiffDisplayRow::FileHeader { file_index } => {
                self.render_file_header(file_index as usize, palette, false, cx)
            }
            DiffDisplayRow::Separator {
                hidden, gap_index, ..
            } => {
                let gap = unpack_diff_row_index(gap_index)
                    .and_then(|index| self.diff_row_context_gaps.get(index).copied());
                self.render_separator(display_index, hidden as usize, gap, palette, cx)
            }
            DiffDisplayRow::Split {
                file_index,
                hunk_index,
                old_line_index,
                new_line_index,
            } => {
                let file_index = file_index as usize;
                let hunk_index = hunk_index as usize;
                let hunk = &self.diff.files[file_index].hunks[hunk_index];
                let content = hunk.content.clone();
                let old = unpack_diff_row_index(old_line_index)
                    .map(|line_index| hunk.lines[line_index].clone());
                let new = unpack_diff_row_index(new_line_index)
                    .map(|line_index| hunk.lines[line_index].clone());
                let old_content = old.as_ref().map(|line| line.content_from(&content));
                let new_content = new.as_ref().map(|line| line.content_from(&content));
                self.render_split_row(
                    display_index,
                    file_index,
                    old.as_ref(),
                    old_content,
                    new.as_ref(),
                    new_content,
                    palette,
                    cx,
                )
            }
            DiffDisplayRow::Unified {
                file_index,
                hunk_index,
                row_index,
                counterpart_index,
            } => {
                let file_index = file_index as usize;
                let hunk_index = hunk_index as usize;
                let hunk = &self.diff.files[file_index].hunks[hunk_index];
                let content = hunk.content.clone();
                let line = hunk.lines[row_index as usize].clone();
                let counterpart = unpack_diff_row_index(counterpart_index)
                    .map(|line_index| hunk.lines[line_index].clone());
                let line_content = line.content_from(&content);
                let counterpart = counterpart
                    .as_ref()
                    .map(|counterpart| (counterpart, counterpart.content_from(&content)));
                self.render_unified_row(
                    display_index,
                    file_index,
                    &line,
                    line_content,
                    counterpart,
                    palette,
                    cx,
                )
            }
            DiffDisplayRow::Empty { .. } => div()
                .h(px(180.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .text_color(palette.faint)
                .child("Binary or empty file — no textual lines to render")
                .into_any_element(),
        }
    }

    pub(super) fn render_separator(
        &self,
        display_index: usize,
        hidden: usize,
        gap: Option<ContextGap>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let gutter_width = diff_gutter_width(self.diff_font_size, self.layout);
        let controls_width = match self.layout {
            DiffLayout::Unified => gutter_width * 2. + px(3.),
            DiffLayout::Split => gutter_width + px(3.),
        };
        let stacked_controls = gap.is_some_and(|gap| gap.position == ContextGapPosition::Middle);
        let row_height = self.diff_offsets().row_height(display_index);
        let controls = gap.map_or_else(
            || {
                vec![
                    div()
                        .text_size(px(11.))
                        .text_color(palette.faint)
                        .child("···")
                        .into_any_element(),
                ]
            },
            |gap| {
                let directions: &[ContextExpandDirection] = match gap.position {
                    ContextGapPosition::Leading => &[ContextExpandDirection::Up],
                    ContextGapPosition::Middle => {
                        &[ContextExpandDirection::Down, ContextExpandDirection::Up]
                    }
                    ContextGapPosition::Trailing => &[ContextExpandDirection::Down],
                };
                directions
                    .iter()
                    .copied()
                    .map(|direction| {
                        let (id, icon_direction) = match direction {
                            ContextExpandDirection::Up => {
                                ("expand-context-up", ExpandIconDirection::Up)
                            }
                            ContextExpandDirection::Down => {
                                ("expand-context-down", ExpandIconDirection::Down)
                            }
                        };
                        div()
                            .id((id, display_index))
                            .h_full()
                            .w_full()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|control| control.bg(palette.hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.expand_context(gap, direction, cx);
                            }))
                            .child(context_expand_icon(
                                icon_direction,
                                palette.muted,
                                self.diff_font_size * (14. / DEFAULT_DIFF_FONT_SIZE),
                            ))
                            .into_any_element()
                    })
                    .collect()
            },
        );

        ContextSeparator::new(display_index, format!("{hidden} unmodified lines"), palette)
            .height(row_height)
            .controls(controls_width, stacked_controls, controls)
            .label_size(self.diff_gutter_font_size())
            .into_any_element()
    }

    pub(super) fn render_empty(&self, palette: Palette) -> gpui::AnyElement {
        EmptyState::new(self.empty_title.clone(), self.empty_detail.clone(), palette)
            .into_any_element()
    }
}
