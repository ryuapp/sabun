use super::super::{
    App, FluentBuilder, InteractiveElement, IntoElement, Palette, ParentElement, Pixels,
    RenderOnce, Rgba, Styled, Window, div,
};

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct DiffGutter {
    id: (&'static str, usize),
    number: Option<u32>,
    palette: Palette,
    background: Rgba,
    line_height: Pixels,
    font_size: Pixels,
    width: Pixels,
    bordered: bool,
}

impl DiffGutter {
    pub(in crate::diff_viewer) const fn new(
        id: (&'static str, usize),
        number: Option<u32>,
        palette: Palette,
        background: Rgba,
        line_height: Pixels,
        font_size: Pixels,
        width: Pixels,
    ) -> Self {
        Self {
            id,
            number,
            palette,
            background,
            line_height,
            font_size,
            width,
            bordered: true,
        }
    }

    pub(in crate::diff_viewer) const fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }
}

impl RenderOnce for DiffGutter {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .w(self.width)
            .flex_none()
            .pr_2()
            .text_right()
            .line_height(self.line_height)
            .text_size(self.font_size)
            .text_color(self.palette.faint)
            .bg(self.background)
            .when(self.bordered, |gutter| {
                gutter.border_r_1().border_color(self.palette.border)
            })
            .child(
                self.number
                    .map(|number| number.to_string())
                    .unwrap_or_default(),
            )
    }
}
