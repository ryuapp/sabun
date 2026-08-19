use super::super::{
    AnyElement, App, FluentBuilder, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Pixels, RenderOnce, ScrollHandle, ScrollWheelEvent, StatefulInteractiveElement,
    Styled, Window, div,
};

type ScrollWheelHandler = Box<dyn Fn(&ScrollWheelEvent, &mut Window, &mut App)>;
type MouseDownHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct ScrollViewport {
    id: &'static str,
    scroll_handle: ScrollHandle,
    scrollbar_width: Pixels,
    content: AnyElement,
    on_scroll_wheel: ScrollWheelHandler,
    on_middle_mouse_down: Option<MouseDownHandler>,
}

impl ScrollViewport {
    pub(in crate::diff_viewer) fn new(
        id: &'static str,
        scroll_handle: ScrollHandle,
        scrollbar_width: Pixels,
        content: impl IntoElement,
        on_scroll_wheel: impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id,
            scroll_handle,
            scrollbar_width,
            content: content.into_any_element(),
            on_scroll_wheel: Box::new(on_scroll_wheel),
            on_middle_mouse_down: None,
        }
    }

    pub(in crate::diff_viewer) fn on_middle_mouse_down(
        mut self,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_middle_mouse_down = Some(Box::new(listener));
        self
    }
}

impl RenderOnce for ScrollViewport {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .scrollbar_width(self.scrollbar_width)
            .track_scroll(&self.scroll_handle)
            .on_scroll_wheel(self.on_scroll_wheel)
            .when_some(self.on_middle_mouse_down, |viewport, listener| {
                viewport.on_mouse_down(MouseButton::Middle, listener)
            })
            .child(self.content)
    }
}
