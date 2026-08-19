use super::super::{
    App, FileChangeKind, FontWeight, IntoElement, Palette, Pixels, RenderOnce, Window, px,
};
use super::change_badge::ChangeBadge;

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct FileChangeBadge {
    kind: FileChangeKind,
    palette: Palette,
    size: Pixels,
    text_size: Pixels,
    font_weight: FontWeight,
}

impl FileChangeBadge {
    pub(in crate::diff_viewer) const fn new(kind: FileChangeKind, palette: Palette) -> Self {
        Self {
            kind,
            palette,
            size: px(20.),
            text_size: px(10.),
            font_weight: FontWeight::MEDIUM,
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

    pub(in crate::diff_viewer) const fn font_weight(mut self, font_weight: FontWeight) -> Self {
        self.font_weight = font_weight;
        self
    }
}

impl RenderOnce for FileChangeBadge {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        ChangeBadge::new(self.kind.label(), self.kind.color(self.palette))
            .size(self.size)
            .text_size(self.text_size)
            .font_weight(self.font_weight)
    }
}
