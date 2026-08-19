use super::super::{
    App, FontWeight, IntoElement, Palette, ParentElement, Pixels, RenderOnce, Styled, Window, div,
    px,
};

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct DiffStats {
    additions: usize,
    deletions: usize,
    gap: Pixels,
    font_weight: FontWeight,
    palette: Palette,
}

impl DiffStats {
    pub(in crate::diff_viewer) const fn new(
        additions: usize,
        deletions: usize,
        palette: Palette,
    ) -> Self {
        Self {
            additions,
            deletions,
            gap: px(8.),
            font_weight: FontWeight::NORMAL,
            palette,
        }
    }

    pub(in crate::diff_viewer) const fn gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self
    }

    pub(in crate::diff_viewer) const fn font_weight(mut self, font_weight: FontWeight) -> Self {
        self.font_weight = font_weight;
        self
    }
}

impl RenderOnce for DiffStats {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(self.gap)
            .text_xs()
            .font_weight(self.font_weight)
            .child(
                div()
                    .text_color(self.palette.green)
                    .child(format!("+{}", self.additions)),
            )
            .child(
                div()
                    .text_color(self.palette.red)
                    .child(format!("−{}", self.deletions)),
            )
    }
}
