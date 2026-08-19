use std::ops::Range;

use super::{
    ClipboardItem, Context, DiffDisplayRow, DiffViewer, FontWeight, MouseDownEvent, MouseMoveEvent,
    Pixels, TextRun, Window, diff_code_inset, font, point, px, unpack_diff_row_index,
    wrapped_code_width,
};

const HEADER_PATH_LEFT_INSET: f32 = 74.;
const HEADER_PATH_FONT_SIZE: f32 = 14.;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TextLane {
    Unified,
    Old,
    New,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TextPoint {
    pub(super) display_index: usize,
    pub(super) lane: TextLane,
    pub(super) offset: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TextSelection {
    pub(super) anchor: TextPoint,
    pub(super) head: TextPoint,
    pub(super) dragging: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HeaderTextSelection {
    pub(super) file_index: usize,
    pub(super) sticky_header: bool,
    pub(super) anchor: usize,
    pub(super) head: usize,
    pub(super) dragging: bool,
}

impl HeaderTextSelection {
    pub(super) fn range(self, text_len: usize) -> Option<Range<usize>> {
        if self.anchor == self.head {
            return None;
        }
        let start = self.anchor.min(self.head).min(text_len);
        let end = self.anchor.max(self.head).min(text_len);
        (start < end).then_some(start..end)
    }
}

impl TextSelection {
    fn ordered(self) -> (TextPoint, TextPoint) {
        if (self.anchor.display_index, self.anchor.offset)
            <= (self.head.display_index, self.head.offset)
        {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub(super) fn range_for(
        self,
        display_index: usize,
        lane: TextLane,
        text_len: usize,
    ) -> Option<Range<usize>> {
        if self.anchor.lane != lane || self.head.lane != lane || self.anchor == self.head {
            return None;
        }
        let (start, end) = self.ordered();
        if display_index < start.display_index || display_index > end.display_index {
            return None;
        }

        let range = if start.display_index == end.display_index {
            start.offset.min(text_len)..end.offset.min(text_len)
        } else if display_index == start.display_index {
            start.offset.min(text_len)..text_len
        } else if display_index == end.display_index {
            0..end.offset.min(text_len)
        } else {
            0..text_len
        };
        Some(range)
    }
}

impl DiffViewer {
    pub(super) fn text_selection_range(
        &self,
        display_index: usize,
        lane: TextLane,
        text_len: usize,
    ) -> Option<Range<usize>> {
        self.text_selection?
            .range_for(display_index, lane, text_len)
    }

    pub(super) fn begin_text_selection(
        &mut self,
        display_index: usize,
        lane: TextLane,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(point) = self.text_point_from_mouse(display_index, lane, event.position, window)
        else {
            return;
        };
        self.header_text_selection = None;
        self.text_selection = Some(TextSelection {
            anchor: point,
            head: point,
            dragging: true,
        });
        cx.notify();
    }

    pub(super) fn begin_header_text_selection(
        &mut self,
        file_index: usize,
        sticky_header: bool,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(offset) = self.header_text_offset(file_index, event.position.x, window) else {
            return;
        };
        self.text_selection = None;
        self.header_text_selection = Some(HeaderTextSelection {
            file_index,
            sticky_header,
            anchor: offset,
            head: offset,
            dragging: true,
        });
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn update_text_selection(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selection) = self
            .header_text_selection
            .filter(|selection| selection.dragging)
        {
            let Some(head) =
                self.header_text_offset(selection.file_index, event.position.x, window)
            else {
                return;
            };
            if head != selection.head {
                self.header_text_selection = Some(HeaderTextSelection { head, ..selection });
                cx.notify();
            }
            return;
        }

        let Some(selection) = self.text_selection.filter(|selection| selection.dragging) else {
            return;
        };

        let bounds = self.diff_scroll.bounds();
        let content_y = event.position.y - bounds.top() - self.diff_scroll.offset().y;
        if content_y < px(0.) || self.diff_rows.is_empty() {
            return;
        }
        let display_index = self
            .diff_offsets()
            .row_index_at(content_y, self.diff_rows.len());
        let Some(point) = self.text_point_from_mouse(
            display_index,
            selection.anchor.lane,
            event.position,
            window,
        ) else {
            return;
        };
        if point != selection.head {
            self.text_selection = Some(TextSelection {
                head: point,
                ..selection
            });
            cx.notify();
        }
    }

    pub(super) fn finish_text_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(selection) = self.header_text_selection {
            if selection.anchor == selection.head {
                self.header_text_selection = None;
                self.toggle_file_collapsed(selection.file_index, selection.sticky_header, cx);
                return;
            } else if selection.dragging {
                self.header_text_selection = Some(HeaderTextSelection {
                    dragging: false,
                    ..selection
                });
            } else {
                return;
            }
            cx.notify();
            return;
        }

        let Some(selection) = self.text_selection else {
            return;
        };
        if selection.anchor == selection.head {
            self.text_selection = None;
        } else if selection.dragging {
            self.text_selection = Some(TextSelection {
                dragging: false,
                ..selection
            });
        } else {
            return;
        }
        cx.notify();
    }

    pub(super) fn copy_text_selection(&self, cx: &mut Context<Self>) {
        let Some(text) = self.selected_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub(super) fn selected_text(&self) -> Option<String> {
        if let Some(selection) = self.header_text_selection {
            let file = self.file_meta.get(selection.file_index)?;
            let range = selection.range(file.header_path.len())?;
            return Some(file.header_path[range].to_owned());
        }

        let selection = self.text_selection?;
        if selection.anchor == selection.head {
            return None;
        }
        let (start, end) = selection.ordered();
        let mut lines = Vec::new();
        for display_index in start.display_index..=end.display_index {
            let Some(content) = self.text_content(display_index, start.lane) else {
                continue;
            };
            let Some(range) = selection.range_for(display_index, start.lane, content.len()) else {
                continue;
            };
            lines.push(content[range].to_owned());
        }
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    pub(super) fn header_text_selection_range(
        &self,
        file_index: usize,
        text_len: usize,
    ) -> Option<Range<usize>> {
        self.header_text_selection
            .filter(|selection| selection.file_index == file_index)?
            .range(text_len)
    }

    fn header_text_offset(
        &self,
        file_index: usize,
        mouse_x: Pixels,
        window: &Window,
    ) -> Option<usize> {
        let file = self.file_meta.get(file_index)?;
        let mut file_name_font = font("Segoe UI");
        file_name_font.weight = FontWeight::SEMIBOLD;
        let runs = [
            TextRun {
                len: file.header_file_name_start,
                font: font("Segoe UI"),
                color: self.palette().muted.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            },
            TextRun {
                len: file
                    .header_path
                    .len()
                    .saturating_sub(file.header_file_name_start),
                font: file_name_font,
                color: self.palette().text.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            },
        ];
        let shaped = window
            .text_system()
            .shape_text(
                file.header_path.clone(),
                px(HEADER_PATH_FONT_SIZE),
                &runs,
                None,
                None,
            )
            .ok()?;
        let local_x =
            (mouse_x - self.diff_scroll.bounds().left() - px(HEADER_PATH_LEFT_INSET)).max(px(0.));
        Some(
            shaped
                .first()?
                .closest_index_for_position(point(local_x, px(0.)), px(20.))
                .unwrap_or_else(|offset| offset)
                .min(file.header_path.len()),
        )
    }

    fn text_point_from_mouse(
        &self,
        display_index: usize,
        lane: TextLane,
        mouse: gpui::Point<Pixels>,
        window: &Window,
    ) -> Option<TextPoint> {
        let content = self.text_content(display_index, lane)?;
        let bounds = self.diff_scroll.bounds();
        let code_width = wrapped_code_width(
            window.viewport_size().width,
            self.sidebar_width,
            self.layout,
            self.diff_font_size,
        );
        let pane_offset = match lane {
            TextLane::Unified | TextLane::Old => px(0.),
            TextLane::New => bounds.size.width / 2.,
        };
        let code_inset = diff_code_inset(self.diff_font_size, self.layout);
        let code_origin_x = bounds.left() + pane_offset + code_inset;
        let row_top =
            bounds.top() + self.diff_offsets().get(display_index)? + self.diff_scroll.offset().y;
        let local = point(
            (mouse.x - code_origin_x).max(px(0.)),
            (mouse.y - row_top - self.diff_code_padding_y()).max(px(0.)),
        );
        let runs = [TextRun {
            len: content.len(),
            font: font("Cascadia Mono"),
            color: self.palette().text.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let shaped = window
            .text_system()
            .shape_text(
                content.to_owned().into(),
                self.diff_font_size,
                &runs,
                Some(code_width),
                None,
            )
            .ok()?;
        let layout = shaped.first()?;
        let offset = layout
            .closest_index_for_position(local, self.diff_code_line_height())
            .unwrap_or_else(|offset| offset)
            .min(content.len());
        Some(TextPoint {
            display_index,
            lane,
            offset,
        })
    }

    fn text_content(&self, display_index: usize, lane: TextLane) -> Option<&str> {
        match (self.diff_rows.get(display_index)?, lane) {
            (
                DiffDisplayRow::Unified {
                    file_index,
                    hunk_index,
                    row_index,
                    ..
                },
                TextLane::Unified,
            ) => {
                let hunk = &self.diff.files[file_index as usize].hunks[hunk_index as usize];
                Some(hunk.line_content(&hunk.lines[row_index as usize]))
            }
            (
                DiffDisplayRow::Split {
                    file_index,
                    hunk_index,
                    old_line_index: line_index,
                    ..
                },
                TextLane::Old,
            )
            | (
                DiffDisplayRow::Split {
                    file_index,
                    hunk_index,
                    new_line_index: line_index,
                    ..
                },
                TextLane::New,
            ) => {
                let line_index = unpack_diff_row_index(line_index)?;
                let hunk = &self.diff.files[file_index as usize].hunks[hunk_index as usize];
                Some(hunk.line_content(&hunk.lines[line_index]))
            }
            _ => None,
        }
    }
}
