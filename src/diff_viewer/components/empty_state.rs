use super::super::{
    App, FontWeight, IntoElement, Palette, ParentElement, RenderOnce, Rgba, SharedString, Styled,
    Window, div, px,
};

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct EmptyState {
    title: SharedString,
    detail: SharedString,
    icon: SharedString,
    icon_color: Rgba,
    icon_background: Rgba,
    palette: Palette,
}

impl EmptyState {
    pub(in crate::diff_viewer) fn new(
        title: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        palette: Palette,
    ) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            icon: "✓".into(),
            icon_color: palette.green,
            icon_background: palette.green_bg,
            palette,
        }
    }

    pub(in crate::diff_viewer) fn icon(
        mut self,
        icon: impl Into<SharedString>,
        color: Rgba,
        background: Rgba,
    ) -> Self {
        self.icon = icon.into();
        self.icon_color = color;
        self.icon_background = background;
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(self.palette.canvas)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .text_color(self.palette.muted)
                    .child(
                        div()
                            .size(px(48.))
                            .rounded_full()
                            .bg(self.icon_background)
                            .text_color(self.icon_color)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xl()
                            .child(self.icon),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(self.palette.text)
                            .child(self.title),
                    )
                    .child(self.detail),
            )
    }
}
