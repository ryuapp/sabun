use super::{
    ClipboardItem, Context, DiffViewer, FluentBuilder, InteractiveElement, IntoElement,
    MouseDownEvent, Palette, ParentElement, Pixels, Point, Size, StatefulInteractiveElement,
    Styled, div, point, px,
};

const CONTEXT_MENU_WIDTH: f32 = 180.;
const CONTEXT_MENU_HEIGHT: f32 = 40.;
const CONTEXT_MENU_MARGIN: f32 = 4.;
const COPY_FEEDBACK_HOLD: super::Duration = super::Duration::from_millis(1_250);
const COPY_FEEDBACK_EXIT: super::Duration = super::Duration::from_millis(180);

pub(super) struct PathContextMenu {
    pub(super) position: Point<Pixels>,
    pub(super) file_index: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CopyPathFeedbackPhase {
    Visible,
    Exiting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CopyPathFeedback {
    pub(super) file_index: usize,
    pub(super) generation: u64,
    pub(super) phase: CopyPathFeedbackPhase,
}

impl DiffViewer {
    pub(super) fn open_text_context_menu(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if self.path_context_menu.is_some() {
            return;
        }
        self.text_context_menu = self.selected_text().map(|_| event.position);
        if self.text_context_menu.is_some() {
            cx.stop_propagation();
        }
        cx.notify();
    }

    pub(super) fn open_path_context_menu(
        &mut self,
        file_index: usize,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if self.file_meta.get(file_index).is_none() {
            return;
        }
        self.text_context_menu = None;
        self.path_context_menu = Some(PathContextMenu {
            position: event.position,
            file_index,
        });
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn copy_file_path(&mut self, file_index: usize, cx: &mut Context<Self>) {
        let Some(path) = self.file_meta.get(file_index).map(|file| {
            file.relative_path
                .as_ref()
                .unwrap_or(&file.display_path)
                .to_string()
        }) else {
            return;
        };
        self.copy_file_path_value(file_index, path, cx);
    }

    fn copy_absolute_file_path(&mut self, file_index: usize, cx: &mut Context<Self>) {
        let Some(path) = self
            .file_meta
            .get(file_index)
            .and_then(|file| file.absolute_path.as_ref())
            .map(ToString::to_string)
        else {
            return;
        };
        self.copy_file_path_value(file_index, path, cx);
    }

    fn copy_relative_file_path(&mut self, file_index: usize, cx: &mut Context<Self>) {
        let Some(path) = self
            .file_meta
            .get(file_index)
            .and_then(|file| file.relative_path.as_ref())
            .map(ToString::to_string)
        else {
            return;
        };
        self.copy_file_path_value(file_index, path, cx);
    }

    fn copy_file_path_value(&mut self, file_index: usize, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path));
        self.copy_path_feedback_generation = self.copy_path_feedback_generation.wrapping_add(1);
        let generation = self.copy_path_feedback_generation;
        self.copy_path_feedback = Some(CopyPathFeedback {
            file_index,
            generation,
            phase: CopyPathFeedbackPhase::Visible,
        });
        cx.notify();

        cx.spawn(async move |viewer, cx| {
            cx.background_executor().timer(COPY_FEEDBACK_HOLD).await;
            let still_current = viewer
                .update(cx, |viewer, cx| {
                    let Some(feedback) = viewer
                        .copy_path_feedback
                        .filter(|feedback| feedback.generation == generation)
                    else {
                        return false;
                    };
                    viewer.copy_path_feedback = Some(CopyPathFeedback {
                        phase: CopyPathFeedbackPhase::Exiting,
                        ..feedback
                    });
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !still_current {
                return;
            }

            cx.background_executor().timer(COPY_FEEDBACK_EXIT).await;
            let _ = viewer.update(cx, |viewer, cx| {
                if viewer
                    .copy_path_feedback
                    .is_some_and(|feedback| feedback.generation == generation)
                {
                    viewer.copy_path_feedback = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn render_text_context_menu(
        &self,
        position: Point<Pixels>,
        viewport: Size<Pixels>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let position = clamp_context_menu_position(position, viewport);
        div()
            .id("text-context-menu")
            .absolute()
            .left(position.x)
            .top(position.y)
            .w(px(CONTEXT_MENU_WIDTH))
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(palette.border)
            .bg(palette.elevated)
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.text_context_menu.take().is_some() {
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("copy-selected-text")
                    .h(px(30.))
                    .px_2()
                    .rounded_sm()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_sm()
                    .text_color(palette.text)
                    .hover(|item| item.bg(palette.hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.copy_text_selection(cx);
                        this.text_context_menu = None;
                        cx.notify();
                    }))
                    .child("Copy")
                    .child(div().text_xs().text_color(palette.faint).child("Ctrl+C")),
            )
            .into_any_element()
    }

    pub(super) fn render_path_context_menu(
        &self,
        menu: &PathContextMenu,
        viewport: Size<Pixels>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let can_copy_selection = self
            .header_text_selection
            .filter(|selection| selection.file_index == menu.file_index)
            .and_then(|selection| {
                self.file_meta
                    .get(menu.file_index)
                    .and_then(|file| selection.range(file.header_path.len()))
            })
            .is_some();
        let Some(file) = self.file_meta.get(menu.file_index) else {
            return div().into_any_element();
        };
        let can_copy_absolute_path = file.absolute_path.is_some();
        let can_copy_relative_path = file.relative_path.is_some();
        let item_count = usize::from(can_copy_selection)
            + usize::from(can_copy_absolute_path)
            + usize::from(can_copy_relative_path);
        let menu_height = context_menu_height(item_count);
        let position =
            clamp_context_menu_position_with_height(menu.position, viewport, menu_height);
        let file_index = menu.file_index;
        div()
            .id("path-context-menu")
            .absolute()
            .left(position.x)
            .top(position.y)
            .w(px(CONTEXT_MENU_WIDTH))
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(palette.border)
            .bg(palette.elevated)
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.path_context_menu.take().is_some() {
                    cx.notify();
                }
            }))
            .when(can_copy_selection, |menu| {
                menu.child(
                    div()
                        .id("copy-selected-header-text")
                        .h(px(30.))
                        .px_2()
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_sm()
                        .text_color(palette.text)
                        .hover(|item| item.bg(palette.hover))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.copy_text_selection(cx);
                            this.path_context_menu = None;
                            cx.notify();
                        }))
                        .child("Copy")
                        .child(div().text_xs().text_color(palette.faint).child("Ctrl+C")),
                )
            })
            .when(can_copy_absolute_path, |menu| {
                menu.child(
                    div()
                        .id("copy-absolute-file-path")
                        .h(px(30.))
                        .px_2()
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .text_sm()
                        .text_color(palette.text)
                        .hover(|item| item.bg(palette.hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.copy_absolute_file_path(file_index, cx);
                            this.path_context_menu = None;
                            cx.notify();
                        }))
                        .child("Copy Path"),
                )
            })
            .when(can_copy_relative_path, |menu| {
                menu.child(
                    div()
                        .id("copy-relative-file-path")
                        .h(px(30.))
                        .px_2()
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .text_sm()
                        .text_color(palette.text)
                        .hover(|item| item.bg(palette.hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.copy_relative_file_path(file_index, cx);
                            this.path_context_menu = None;
                            cx.notify();
                        }))
                        .child("Copy relative path"),
                )
            })
            .into_any_element()
    }
}

const fn context_menu_height(item_count: usize) -> f32 {
    32.0f32.mul_add(item_count as f32, 8.)
}

pub(super) fn clamp_context_menu_position(
    position: Point<Pixels>,
    viewport: Size<Pixels>,
) -> Point<Pixels> {
    clamp_context_menu_position_with_height(position, viewport, CONTEXT_MENU_HEIGHT)
}

fn clamp_context_menu_position_with_height(
    position: Point<Pixels>,
    viewport: Size<Pixels>,
    height: f32,
) -> Point<Pixels> {
    let margin = px(CONTEXT_MENU_MARGIN);
    point(
        position.x.clamp(
            margin,
            (viewport.width - px(CONTEXT_MENU_WIDTH) - margin).max(margin),
        ),
        position
            .y
            .clamp(margin, (viewport.height - px(height) - margin).max(margin)),
    )
}
