use super::super::{
    App, ClickEvent, FluentBuilder, InteractiveElement, IntoElement, Palette, ParentElement,
    RenderOnce, StatefulInteractiveElement, Styled, Window, div,
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct SegmentedButton {
    id: &'static str,
    label: &'static str,
    active: bool,
    palette: Palette,
    on_click: ClickHandler,
}

impl SegmentedButton {
    pub(in crate::diff_viewer) fn new(
        id: &'static str,
        label: &'static str,
        active: bool,
        palette: Palette,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id,
            label,
            active,
            palette,
            on_click: Box::new(on_click),
        }
    }
}

impl RenderOnce for SegmentedButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .rounded_sm()
            .px_2()
            .py_1()
            .text_xs()
            .text_color(if self.active {
                self.palette.text
            } else {
                self.palette.faint
            })
            .when(self.active, |button| button.bg(self.palette.selection))
            .on_click(self.on_click)
            .child(self.label)
    }
}
