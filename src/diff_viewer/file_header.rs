use super::{
    Animation, AnimationExt, Context, CopyPathFeedbackPhase, CursorStyle, DiffStats, DiffViewer,
    Duration, FILE_HEADER_HEIGHT, FileChangeBadge, FluentBuilder, FontWeight, HighlightStyle,
    InteractiveElement, IntoElement, MouseButton, Palette, ParentElement, SCROLLBAR_WIDTH, Styled,
    StyledText, combine_highlights, div, ease_out_quint, px,
};
use crate::icons::{DISCLOSURE_ICON_SIZE, check_icon, copy_icon, disclosure_icon};

impl DiffViewer {
    pub(super) fn render_file_header(
        &self,
        file_index: usize,
        palette: Palette,
        sticky: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(file) = self.file_meta.get(file_index) else {
            return div().into_any_element();
        };
        let collapsed = self.collapsed_files.contains(&file_index);
        let viewed = self.is_file_viewed(file_index);
        let (additions, deletions) = self.file_stats.get(file_index).copied().unwrap_or_default();
        let file_name_end = file.header_path.len();
        let file_name_highlight = [(
            file.header_file_name_start..file_name_end,
            HighlightStyle {
                color: Some(palette.text.into()),
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            },
        )];
        let selection = self
            .header_text_selection_range(file_index, file.header_path.len())
            .into_iter()
            .map(|range| {
                (
                    range,
                    HighlightStyle {
                        background_color: Some(palette.selection.into()),
                        ..Default::default()
                    },
                )
            });
        let display_path = StyledText::new(file.header_path.clone())
            .with_highlights(combine_highlights(file_name_highlight, selection));
        let copy_button_icon = self
            .copy_path_feedback
            .filter(|feedback| feedback.file_index == file_index)
            .map_or_else(
                || copy_icon(palette.muted, px(16.)).into_any_element(),
                |feedback| {
                    let exiting = feedback.phase == CopyPathFeedbackPhase::Exiting;
                    div()
                        .size(px(16.))
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .size(px(16.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(copy_icon(palette.muted, px(16.)))
                                .with_animation(
                                    (
                                        match (sticky, exiting) {
                                            (true, true) => "sticky-copy-path-icon-in",
                                            (true, false) => "sticky-copy-path-icon-out",
                                            (false, true) => "copy-path-icon-in",
                                            (false, false) => "copy-path-icon-out",
                                        },
                                        feedback.generation,
                                    ),
                                    Animation::new(Duration::from_millis(180))
                                        .with_easing(ease_out_quint()),
                                    move |icon, delta| {
                                        icon.opacity(if exiting { delta } else { 1. - delta })
                                    },
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .size(px(16.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(check_icon(palette.muted, px(16.)))
                                .with_animation(
                                    (
                                        match (sticky, exiting) {
                                            (true, true) => "sticky-check-path-icon-out",
                                            (true, false) => "sticky-check-path-icon-in",
                                            (false, true) => "check-path-icon-out",
                                            (false, false) => "check-path-icon-in",
                                        },
                                        feedback.generation,
                                    ),
                                    Animation::new(Duration::from_millis(180))
                                        .with_easing(ease_out_quint()),
                                    move |icon, delta| {
                                        let offset = if exiting {
                                            px(-2. * delta)
                                        } else {
                                            px(2. * (1. - delta))
                                        };
                                        icon.relative().top(offset).opacity(if exiting {
                                            1. - delta
                                        } else {
                                            delta
                                        })
                                    },
                                ),
                        )
                        .into_any_element()
                },
            );

        div()
            .id((
                if sticky {
                    "sticky-diff-file-header"
                } else {
                    "diff-file-header"
                },
                file_index,
            ))
            .h(px(FILE_HEADER_HEIGHT))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .pl_4()
            .pr(px(16. + SCROLLBAR_WIDTH))
            .border_b_1()
            .border_color(palette.border)
            .bg(palette.panel)
            .cursor(CursorStyle::Arrow)
            .font_family("Segoe UI")
            .child(
                div()
                    .w(px(0.))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w(px(DISCLOSURE_ICON_SIZE))
                            .flex_none()
                            .child(disclosure_icon(!collapsed, palette.muted)),
                    )
                    .child(
                        FileChangeBadge::new(file.change_kind, palette)
                            .size(px(22.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .id((
                                        if sticky {
                                            "select-sticky-file-header-path"
                                        } else {
                                            "select-file-header-path"
                                        },
                                        file_index,
                                    ))
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_overflow(gpui::TextOverflow::Truncate("...".into()))
                                    .text_sm()
                                    .text_color(palette.muted)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, event, window, cx| {
                                            this.begin_header_text_selection(
                                                file_index, sticky, event, window, cx,
                                            );
                                        }),
                                    )
                                    .child(display_path),
                            )
                            .child(
                                div()
                                    .id((
                                        if sticky {
                                            "copy-sticky-file-header-path"
                                        } else {
                                            "copy-file-header-path"
                                        },
                                        file_index,
                                    ))
                                    .size(px(28.))
                                    .flex_none()
                                    .rounded_sm()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(|button| button.bg(palette.hover))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.copy_file_path(file_index, cx);
                                        }),
                                    )
                                    .child(copy_button_icon),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        DiffStats::new(additions, deletions, palette)
                            .font_weight(FontWeight::MEDIUM),
                    )
                    .child(
                        div()
                            .id((
                                if sticky {
                                    "sticky-file-viewed"
                                } else {
                                    "file-viewed"
                                },
                                file_index,
                            ))
                            .h(px(28.))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_md()
                            .border_1()
                            .border_color(palette.border)
                            .bg(palette.elevated)
                            .text_xs()
                            .text_color(if viewed { palette.green } else { palette.muted })
                            .hover(|button| button.bg(palette.hover))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_file_viewed(file_index, cx);
                                }),
                            )
                            .child(
                                div()
                                    .size(px(15.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(if viewed {
                                        palette.green
                                    } else {
                                        palette.muted
                                    })
                                    .when(viewed, |checkbox| {
                                        checkbox
                                            .bg(palette.green)
                                            .child(check_icon(palette.canvas, px(13.)))
                                    }),
                            )
                            .child("Viewed"),
                    ),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event, _, cx| {
                    this.open_path_context_menu(file_index, event, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.toggle_file_collapsed(file_index, sticky, cx);
                }),
            )
            .into_any_element()
    }
}
