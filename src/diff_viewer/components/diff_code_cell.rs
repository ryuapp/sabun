use super::super::{
    AnyElement, App, CursorStyle, FluentBuilder, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Pixels, RenderOnce, Styled, Window, div,
};

type MouseDownHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct DiffCodeCell {
    id: (&'static str, usize),
    code: AnyElement,
    selection_padding: Option<AnyElement>,
    padding_y: Pixels,
    line_height: Pixels,
    on_mouse_down: Option<MouseDownHandler>,
}

impl DiffCodeCell {
    pub(in crate::diff_viewer) fn new(
        id: (&'static str, usize),
        code: impl IntoElement,
        padding_y: Pixels,
        line_height: Pixels,
    ) -> Self {
        Self {
            id,
            code: code.into_any_element(),
            selection_padding: None,
            padding_y,
            line_height,
            on_mouse_down: None,
        }
    }

    pub(in crate::diff_viewer) fn selection_padding(
        mut self,
        selection_padding: Option<impl IntoElement>,
    ) -> Self {
        self.selection_padding = selection_padding.map(IntoElement::into_any_element);
        self
    }

    pub(in crate::diff_viewer) fn on_mouse_down(
        mut self,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_mouse_down = Some(Box::new(listener));
        self
    }
}

impl RenderOnce for DiffCodeCell {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div().id(self.id).min_w_0().flex_1().pr_3().child(
            div()
                .relative()
                .w_full()
                .py(self.padding_y)
                .line_height(self.line_height)
                .whitespace_normal()
                .when_some(self.on_mouse_down, |container, listener| {
                    container
                        .cursor(CursorStyle::IBeam)
                        .on_mouse_down(MouseButton::Left, listener)
                })
                .when_some(self.selection_padding, |container, overlay| {
                    container.child(overlay)
                })
                .child(self.code),
        )
    }
}
