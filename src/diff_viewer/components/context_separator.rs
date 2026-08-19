use super::super::{
    AnyElement, App, FluentBuilder, InteractiveElement, IntoElement, Palette, ParentElement,
    Pixels, RenderOnce, SharedString, Styled, Window, div, px,
};

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct ContextSeparator {
    display_index: usize,
    height: Pixels,
    controls_width: Pixels,
    controls_stacked: bool,
    controls: Vec<AnyElement>,
    label: SharedString,
    label_size: Pixels,
    palette: Palette,
}

impl ContextSeparator {
    pub(in crate::diff_viewer) fn new(
        display_index: usize,
        label: impl Into<SharedString>,
        palette: Palette,
    ) -> Self {
        Self {
            display_index,
            height: px(21.),
            controls_width: px(0.),
            controls_stacked: false,
            controls: Vec::new(),
            label: label.into(),
            label_size: px(10.),
            palette,
        }
    }

    pub(in crate::diff_viewer) const fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub(in crate::diff_viewer) fn controls(
        mut self,
        width: Pixels,
        stacked: bool,
        controls: Vec<AnyElement>,
    ) -> Self {
        self.controls_width = width;
        self.controls_stacked = stacked;
        self.controls = controls;
        self
    }

    pub(in crate::diff_viewer) const fn label_size(mut self, label_size: Pixels) -> Self {
        self.label_size = label_size;
        self
    }
}

impl RenderOnce for ContextSeparator {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id(("separator", self.display_index))
            .h(self.height)
            .w_full()
            .min_w_0()
            .flex_none()
            .overflow_hidden()
            .flex()
            .items_center()
            .border_y_1()
            .border_color(self.palette.border)
            .bg(self.palette.canvas)
            .child(
                div()
                    .h_full()
                    .w(self.controls_width)
                    .flex_none()
                    .overflow_hidden()
                    .flex()
                    .when(self.controls_stacked, gpui::Styled::flex_col)
                    .items_center()
                    .justify_center()
                    .bg(self.palette.canvas)
                    .border_r_1()
                    .border_color(self.palette.border)
                    .children(self.controls),
            )
            .child(
                div()
                    .h_full()
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .px_2()
                    .text_size(self.label_size)
                    .text_color(self.palette.muted)
                    .child(self.label),
            )
    }
}
