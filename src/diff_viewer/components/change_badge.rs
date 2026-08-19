use super::super::{
    App, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, Styled, Window, div, px,
    with_alpha,
};

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct ChangeBadge {
    label: &'static str,
    color: Rgba,
    size: Pixels,
    text_size: Pixels,
    weight: FontWeight,
}

impl ChangeBadge {
    pub(in crate::diff_viewer) const fn new(label: &'static str, color: Rgba) -> Self {
        Self {
            label,
            color,
            size: px(20.),
            text_size: px(10.),
            weight: FontWeight::MEDIUM,
        }
    }

    pub(in crate::diff_viewer) const fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub(in crate::diff_viewer) const fn text_size(mut self, text_size: Pixels) -> Self {
        self.text_size = text_size;
        self
    }

    pub(in crate::diff_viewer) const fn font_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }
}

impl RenderOnce for ChangeBadge {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .size(self.size)
            .flex_none()
            .rounded_sm()
            .bg(with_alpha(self.color, 0.12))
            .flex()
            .items_center()
            .justify_center()
            .text_size(self.text_size)
            .font_weight(self.weight)
            .text_color(self.color)
            .child(self.label)
    }
}
