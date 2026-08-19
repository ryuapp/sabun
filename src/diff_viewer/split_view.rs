use std::{ops::Range, sync::Arc};

use super::code_text::selection_padding_overlay;
use super::{
    Context, DiffChangeBar, DiffCodeCell, DiffGutter, DiffLine, DiffLineAppearance, DiffMarker,
    DiffViewer, InteractiveElement, IntoElement, LineKind, Palette, ParentElement, Side, Styled,
    SyntaxPosition, TextLane, diff_gutter_width, div, px,
};

impl DiffViewer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_split_row(
        &mut self,
        display_index: usize,
        file_index: usize,
        hunk_index: usize,
        old: Option<&DiffLine>,
        old_content: Option<&str>,
        new: Option<&DiffLine>,
        new_content: Option<&str>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (old_inline, new_inline) = match (old, new) {
            (Some(old), Some(new))
                if old.kind == LineKind::Deletion && new.kind == LineKind::Addition =>
            {
                let (old_ranges, new_ranges) = self.inline_pair(
                    display_index,
                    old_content.unwrap_or_default(),
                    new_content.unwrap_or_default(),
                );
                (Some(old_ranges), Some(new_ranges))
            }
            _ => (None, None),
        };
        let row_height = self.diff_offsets().row_height(display_index);
        div()
            .flex_none()
            .h(row_height)
            .flex()
            .child(self.render_split_cell(
                display_index,
                file_index,
                hunk_index,
                Side::Old,
                old,
                old_content,
                old_inline,
                palette,
                cx,
            ))
            .child(self.render_split_cell(
                display_index,
                file_index,
                hunk_index,
                Side::New,
                new,
                new_content,
                new_inline,
                palette,
                cx,
            ))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_split_cell(
        &mut self,
        display_index: usize,
        file_index: usize,
        hunk_index: usize,
        side: Side,
        line: Option<&DiffLine>,
        content: Option<&str>,
        inline_ranges: Option<Arc<[Range<usize>]>>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (number, kind) = line.map_or((None, None), |line| {
            (
                match side {
                    Side::Old => line.old_number,
                    Side::New => line.new_number,
                },
                Some(line.kind),
            )
        });
        let content = content.unwrap_or_default();
        let appearance = DiffLineAppearance::for_kind(kind, palette);
        let background = appearance.background;
        let language = self.file_meta[file_index].language;
        let text_lane = match side {
            Side::Old => TextLane::Old,
            Side::New => TextLane::New,
        };
        let text_selection = self.text_selection_range(display_index, text_lane, content.len());
        let code = self.code_text(
            display_index,
            side,
            content,
            language,
            inline_ranges,
            kind,
            text_selection.clone(),
            palette,
            number.map(|line_number| SyntaxPosition {
                file_index,
                hunk_index,
                line_number,
            }),
        );
        let element_index = display_index;
        let cell_id = match side {
            Side::Old => ("old-cell", element_index),
            Side::New => ("new-cell", element_index),
        };
        let gutter_id = match side {
            Side::Old => ("old-line-number", element_index),
            Side::New => ("new-line-number", element_index),
        };
        let code_id = match side {
            Side::Old => ("old-code", element_index),
            Side::New => ("new-code", element_index),
        };
        let selection_padding = text_selection.and_then(|selection| {
            selection_padding_overlay(
                content,
                selection,
                palette.selection,
                self.diff_font_size,
                self.diff_code_line_height(),
                self.diff_code_padding_y(),
            )
        });
        let code_container = DiffCodeCell::new(
            code_id,
            code,
            self.diff_code_padding_y(),
            self.diff_code_line_height(),
        )
        .selection_padding(selection_padding);
        let code_container = if line.is_some() {
            code_container.on_mouse_down(cx.listener(move |this, event, window, cx| {
                this.begin_text_selection(display_index, text_lane, event, window, cx);
            }))
        } else {
            code_container
        };

        div()
            .id(cell_id)
            .w_1_2()
            .h_full()
            .flex()
            .bg(background)
            .border_r_1()
            .border_color(palette.border)
            .child(DiffChangeBar::new(appearance.marker_background))
            .child(
                DiffGutter::new(
                    gutter_id,
                    number,
                    palette,
                    background,
                    self.diff_code_row_height(),
                    self.diff_gutter_font_size(),
                    diff_gutter_width(self.diff_font_size, self.layout),
                )
                .bordered(false),
            )
            .child(DiffMarker::new(
                appearance,
                px(25.),
                self.diff_code_row_height(),
            ))
            .child(code_container)
            .into_any_element()
    }
}
