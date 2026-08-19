use super::super::{
    App, IntoElement, LineKind, Palette, ParentElement, Pixels, RenderOnce, Rgba, Styled, Window,
    div, px,
};

#[derive(Clone, Copy)]
pub(in crate::diff_viewer) struct DiffLineAppearance {
    pub(in crate::diff_viewer) background: Rgba,
    pub(in crate::diff_viewer) marker_background: Rgba,
    pub(in crate::diff_viewer) marker_color: Rgba,
    pub(in crate::diff_viewer) marker: &'static str,
}

impl DiffLineAppearance {
    pub(in crate::diff_viewer) const fn for_kind(kind: Option<LineKind>, palette: Palette) -> Self {
        let (background, marker_color, marker) = match kind {
            Some(LineKind::Addition) => (palette.green_bg, palette.green, "+"),
            Some(LineKind::Deletion) => (palette.red_bg, palette.red, "−"),
            Some(LineKind::Context) | None => (palette.canvas, palette.faint, " "),
        };
        let marker_background = match kind {
            Some(LineKind::Addition | LineKind::Deletion) => marker_color,
            Some(LineKind::Context) | None => background,
        };
        Self {
            background,
            marker_background,
            marker_color,
            marker,
        }
    }
}

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct DiffChangeBar {
    color: Rgba,
}

impl DiffChangeBar {
    pub(in crate::diff_viewer) const fn new(color: Rgba) -> Self {
        Self { color }
    }
}

impl RenderOnce for DiffChangeBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div().w(px(3.)).flex_none().bg(self.color)
    }
}

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct DiffMarker {
    marker: &'static str,
    color: Rgba,
    width: Pixels,
    line_height: Pixels,
}

impl DiffMarker {
    pub(in crate::diff_viewer) const fn new(
        appearance: DiffLineAppearance,
        width: Pixels,
        line_height: Pixels,
    ) -> Self {
        Self {
            marker: appearance.marker,
            color: appearance.marker_color,
            width,
            line_height,
        }
    }
}

impl RenderOnce for DiffMarker {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .w(self.width)
            .flex_none()
            .text_center()
            .line_height(self.line_height)
            .text_color(self.color)
            .child(self.marker)
    }
}
