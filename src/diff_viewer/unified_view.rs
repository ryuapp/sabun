use super::code_text::selection_padding_overlay;
use super::{
    Context, DiffChangeBar, DiffCodeCell, DiffGutter, DiffLine, DiffLineAppearance, DiffMarker,
    DiffViewer, InteractiveElement, IntoElement, LineKind, Palette, ParentElement, Side, Styled,
    SyntaxPosition, TextLane, diff_gutter_width, div, px,
};

impl DiffViewer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_unified_row(
        &mut self,
        display_index: usize,
        file_index: usize,
        line: &DiffLine,
        content: &str,
        counterpart: Option<(&DiffLine, &str)>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let side = if line.kind == LineKind::Deletion {
            Side::Old
        } else {
            Side::New
        };
        let appearance = DiffLineAppearance::for_kind(Some(line.kind), palette);
        let background = appearance.background;
        let inline = counterpart
            .filter(|(other, _)| other.kind != line.kind)
            .map(|(_, other_content)| {
                if line.kind == LineKind::Deletion {
                    self.inline_pair(display_index, content, other_content).0
                } else {
                    self.inline_pair(display_index, other_content, content).1
                }
            });
        let text_selection =
            self.text_selection_range(display_index, TextLane::Unified, content.len());
        let code = self.code_text(
            display_index,
            side,
            content,
            self.file_meta[file_index].language,
            inline,
            Some(line.kind),
            text_selection.clone(),
            palette,
            Some(SyntaxPosition {
                file_index,
                line_number: if side == Side::Old {
                    line.old_number.expect("old diff line number")
                } else {
                    line.new_number.expect("new diff line number")
                },
            }),
        );
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
        let element_index = display_index;
        let row_height = self.diff_offsets().row_height(display_index);

        div()
            .id(("unified", element_index))
            .flex_none()
            .h(row_height)
            .flex()
            .bg(background)
            .child(DiffChangeBar::new(appearance.marker_background))
            .child(DiffGutter::new(
                ("old-gutter", element_index),
                line.old_number,
                palette,
                background,
                self.diff_code_row_height(),
                self.diff_gutter_font_size(),
                diff_gutter_width(self.diff_font_size, self.layout),
            ))
            .child(DiffGutter::new(
                ("new-gutter", element_index),
                line.new_number,
                palette,
                background,
                self.diff_code_row_height(),
                self.diff_gutter_font_size(),
                diff_gutter_width(self.diff_font_size, self.layout),
            ))
            .child(DiffMarker::new(
                appearance,
                px(28.),
                self.diff_code_row_height(),
            ))
            .child(
                DiffCodeCell::new(
                    ("unified-code", element_index),
                    code,
                    self.diff_code_padding_y(),
                    self.diff_code_line_height(),
                )
                .selection_padding(selection_padding)
                .on_mouse_down(cx.listener(move |this, event, window, cx| {
                    this.begin_text_selection(display_index, TextLane::Unified, event, window, cx);
                })),
            )
            .into_any_element()
    }
}
