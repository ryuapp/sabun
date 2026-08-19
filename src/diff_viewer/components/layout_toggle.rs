use super::super::{
    App, ClickEvent, IntoElement, Palette, ParentElement, RenderOnce, Styled, Window, div, px,
};
use super::SegmentedButton;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct LayoutToggle {
    split: bool,
    palette: Palette,
    on_unified: ClickHandler,
    on_split: ClickHandler,
}

impl LayoutToggle {
    pub(in crate::diff_viewer) fn new(
        split: bool,
        palette: Palette,
        on_unified: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_split: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            split,
            palette,
            on_unified: Box::new(on_unified),
            on_split: Box::new(on_split),
        }
    }
}

impl RenderOnce for LayoutToggle {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .rounded_md()
            .border_1()
            .border_color(self.palette.border)
            .bg(self.palette.sidebar)
            .p(px(2.))
            .child(SegmentedButton::new(
                "layout-unified",
                "Unified",
                !self.split,
                self.palette,
                self.on_unified,
            ))
            .child(SegmentedButton::new(
                "layout-split",
                "Split",
                self.split,
                self.palette,
                self.on_split,
            ))
    }
}
