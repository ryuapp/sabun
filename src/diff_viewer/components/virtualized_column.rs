use super::super::{
    AnyElement, App, IntoElement, ParentElement, Pixels, RenderOnce, Styled, Window, div, px,
};

#[derive(IntoElement)]
pub(in crate::diff_viewer) struct VirtualizedColumn {
    rows: Vec<AnyElement>,
    top_space: Pixels,
    bottom_space: Pixels,
}

impl VirtualizedColumn {
    pub(in crate::diff_viewer) const fn new(
        rows: Vec<AnyElement>,
        top_space: Pixels,
        bottom_space: Pixels,
    ) -> Self {
        Self {
            rows,
            top_space,
            bottom_space,
        }
    }
}

impl RenderOnce for VirtualizedColumn {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut children = Vec::with_capacity(self.rows.len() + 2);
        if self.top_space > px(0.) {
            children.push(div().h(self.top_space).flex_none().into_any_element());
        }
        children.extend(self.rows);
        if self.bottom_space > px(0.) {
            children.push(div().h(self.bottom_space).flex_none().into_any_element());
        }

        div()
            .w_full()
            .min_w_0()
            .flex_none()
            .flex()
            .flex_col()
            .children(children)
    }
}
