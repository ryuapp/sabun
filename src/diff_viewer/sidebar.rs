use super::{
    Context, CursorStyle, DiffViewer, FILE_SCROLLBAR_WIDTH, FileChangeBadge, FileTreeRow,
    FluentBuilder, FontWeight, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, Palette, ParentElement, SIDEBAR_RESIZE_HANDLE_WIDTH, ScrollViewport,
    ScrollbarAxis, ScrollbarTarget, SharedString, SidebarResizeDrag, SmoothScrollTarget,
    StatefulInteractiveElement, Styled, TREE_DIRECTORY_ROW_HEIGHT, TREE_FILE_ROW_HEIGHT,
    TREE_SCROLLBAR_GAP, VirtualizedColumn, Window, clamped_sidebar_width, div, language_color, px,
    sidebar_file_name_width, sticky_file_tree_directories, variable_visible_range, with_alpha,
};
use crate::icons::{DISCLOSURE_ICON_SIZE, disclosure_icon};

impl DiffViewer {
    pub(super) fn start_sidebar_resize(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.cancel_middle_auto_scroll(cx);
        self.sidebar_resize_drag = Some(SidebarResizeDrag {
            pointer_x: event.position.x,
            width: self.sidebar_width,
        });
        cx.notify();
    }

    pub(super) fn update_sidebar_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.sidebar_resize_drag else {
            return;
        };
        if !event.dragging() {
            self.finish_sidebar_resize(cx);
            return;
        }

        let proposed = drag.width + event.position.x - drag.pointer_x;
        let width = clamped_sidebar_width(proposed, window.viewport_size().width);
        if width != self.sidebar_width {
            self.sidebar_width = width;
            cx.notify();
        }
    }

    pub(super) fn finish_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_resize_drag.take().is_none() {
            return;
        }
        self.wrapped_offsets_range = None;
        cx.notify();
    }

    pub(super) fn render_sidebar(
        &mut self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let file_count = self.diff.files.len();
        let (visible_rows, top_space, bottom_space) =
            variable_visible_range(&self.file_tree_row_offsets, &self.file_scroll, 72.);
        let mut file_rows = Vec::with_capacity(visible_rows.len());
        for row_index in visible_rows {
            let row = self.file_tree_rows[row_index].clone();
            file_rows.push(self.render_file_tree_row(&row, None, palette, cx));
        }
        let show_file_scrollbar = self.file_sidebar_hovered
            || self
                .scrollbar_drag
                .is_some_and(|drag| drag.target == ScrollbarTarget::Files);
        let file_scrollbar = show_file_scrollbar.then(|| {
            self.render_scrollbar(
                ScrollbarTarget::Files,
                ScrollbarAxis::Vertical,
                self.file_scroll.clone(),
                palette,
                cx,
            )
        });
        let sticky_directories = sticky_file_tree_directories(
            &self.file_tree_rows,
            &self.file_tree_row_offsets,
            (-self.file_scroll.offset().y).max(px(0.)),
        );
        let sticky_rows = sticky_directories
            .iter()
            .enumerate()
            .map(|(index, row)| {
                self.render_file_tree_row(
                    row,
                    Some(px(index as f32 * TREE_DIRECTORY_ROW_HEIGHT)),
                    palette,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        div()
            .id("file-sidebar")
            .w(self.sidebar_width)
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .relative()
            .bg(palette.sidebar)
            .child(
                div()
                    .px_4()
                    .pt_4()
                    .pb_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text)
                            .child("Changes")
                            .child(
                                div()
                                    .rounded_full()
                                    .bg(palette.elevated)
                                    .px_2()
                                    .text_xs()
                                    .text_color(palette.muted)
                                    .child(file_count.to_string()),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        ScrollViewport::new(
                            "file-scroll",
                            self.file_scroll.clone(),
                            px(FILE_SCROLLBAR_WIDTH),
                            VirtualizedColumn::new(file_rows, top_space, bottom_space),
                            cx.listener(|this, event, window, cx| {
                                this.scroll_wheel(SmoothScrollTarget::Files, event, window, cx);
                            }),
                        )
                        .on_middle_mouse_down(cx.listener(
                            |this, event, window, cx| {
                                this.toggle_middle_auto_scroll(
                                    SmoothScrollTarget::Files,
                                    event,
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
                    .children(sticky_rows)
                    .children(file_scrollbar),
            )
            .child(
                div()
                    .flex_none()
                    .border_t_1()
                    .border_color(palette.border)
                    .px_4()
                    .py_3()
                    .text_xs()
                    .text_color(palette.faint)
                    .flex()
                    .items_center()
                    .justify_end()
                    .child("↑ ↓ navigate"),
            )
            .child(
                div()
                    .id("file-sidebar-hover-sensor")
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .left_0()
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        if this.file_sidebar_hovered != *hovered {
                            this.file_sidebar_hovered = *hovered;
                            cx.notify();
                        }
                    })),
            )
            .into_any_element()
    }

    pub(super) fn render_sidebar_resize_handle(
        &self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .id("sidebar-resize-handle")
            .w(px(SIDEBAR_RESIZE_HANDLE_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(|handle| handle.bg(with_alpha(palette.accent, 0.18)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.start_sidebar_resize(event, cx)),
            )
            .child(
                div()
                    .w(px(1.))
                    .h_full()
                    .bg(if self.sidebar_resize_drag.is_some() {
                        palette.accent
                    } else {
                        palette.border
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_file_tree_row(
        &mut self,
        row: &FileTreeRow,
        sticky_top: Option<gpui::Pixels>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match row {
            FileTreeRow::Directory {
                path,
                name,
                depth,
                expanded,
            } => {
                let path = path.clone();
                let indent = px((*depth as f32).mul_add(14., 10.));
                let sticky = sticky_top.is_some();
                div()
                    .id(SharedString::from(if sticky {
                        format!("sticky-directory:{path}")
                    } else {
                        format!("directory:{path}")
                    }))
                    .h(px(TREE_DIRECTORY_ROW_HEIGHT))
                    .flex_none()
                    .mr(px(FILE_SCROLLBAR_WIDTH))
                    .flex()
                    .items_center()
                    .pr(px(FILE_SCROLLBAR_WIDTH + TREE_SCROLLBAR_GAP))
                    .text_sm()
                    .text_color(palette.muted)
                    .when_some(sticky_top, |row, top| {
                        row.absolute()
                            .top(top)
                            .left_0()
                            .right_0()
                            .bg(palette.sidebar)
                    })
                    .hover(|row| row.bg(palette.hover).text_color(palette.text))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_directory(path.to_string(), cx);
                        }),
                    )
                    .child(div().w(indent).flex_none())
                    .child(
                        div()
                            .w(px(DISCLOSURE_ICON_SIZE))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(disclosure_icon(*expanded, palette.faint)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                    .into_any_element()
            }
            FileTreeRow::File { file_index, depth } => {
                self.render_file_row(*file_index, *depth, palette, cx)
            }
        }
    }

    pub(super) fn render_file_row(
        &mut self,
        index: usize,
        depth: usize,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(file_meta) = self.file_meta.get(index) else {
            return div().into_any_element();
        };
        let selected = index == self.selected_file;
        let icon_color = language_color(file_meta.language);
        let indent = px((depth as f32).mul_add(14., 12.));
        let file_name_width = sidebar_file_name_width(self.sidebar_width, depth);
        let file_name = file_meta.file_name.clone();

        div()
            .id(("file", index))
            .h(px(TREE_FILE_ROW_HEIGHT))
            .flex_none()
            .mr(px(FILE_SCROLLBAR_WIDTH))
            .pr_2()
            .flex()
            .items_center()
            .gap_2()
            .when(selected, |row| row.bg(palette.hover))
            .hover(|row| row.bg(palette.hover))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event, _, cx| {
                    this.open_path_context_menu(index, event, cx);
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select_file(index, cx)))
            .child(div().w(indent).flex_none())
            .child(div().size(px(8.)).rounded_full().bg(icon_color))
            .child(
                div()
                    .w(file_name_width)
                    .flex_none()
                    .flex()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_overflow(gpui::TextOverflow::Truncate("...".into()))
                    .text_sm()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(palette.text)
                    .child(file_name),
            )
            .child(
                FileChangeBadge::new(file_meta.change_kind, palette)
                    .size(px(20.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .into_any_element()
    }
}
