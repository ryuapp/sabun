use super::{
    Context, DiffLayout, DiffStats, DiffViewer, FluentBuilder, FontWeight, InteractiveElement,
    IntoElement, LayoutToggle, Palette, ParentElement, StatefulInteractiveElement, Styled,
    ThemeIcon, ThemeMode, div, px, theme_icon,
};

impl DiffViewer {
    pub(super) fn render_top_bar(
        &mut self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let total_additions = self.total_additions;
        let total_deletions = self.total_deletions;
        let is_split = self.layout == DiffLayout::Split;
        let working_tree = self.target_label == "working tree";
        let index = self.target_label == "index";
        div()
            .h(px(56.))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .border_b_1()
            .border_color(palette.border)
            .bg(palette.panel)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .child(
                        div()
                            .text_color(palette.text)
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.source_name.clone()),
                    )
                    .child(div().text_color(palette.faint).child("/"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(palette.text)
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(self.comparison_label.clone()),
                            )
                            .when(working_tree || index, |comparison| {
                                comparison
                                    .child(div().text_color(palette.faint).child("·"))
                                    .child(div().text_color(palette.muted).child(if working_tree {
                                        "uncommitted"
                                    } else {
                                        "staged"
                                    }))
                            })
                            .when(!working_tree && !index, |comparison| {
                                comparison
                                    .child(div().text_color(palette.faint).child("→"))
                                    .child(
                                        div()
                                            .text_color(palette.muted)
                                            .child(self.target_label.clone()),
                                    )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(DiffStats::new(total_additions, total_deletions, palette).gap(px(12.)))
                    .child(LayoutToggle::new(
                        is_split,
                        palette,
                        cx.listener(|this, _, _, cx| {
                            this.set_layout(DiffLayout::Unified, cx);
                        }),
                        cx.listener(|this, _, _, cx| {
                            this.set_layout(DiffLayout::Split, cx);
                        }),
                    ))
                    .child(
                        div()
                            .id("theme-toggle")
                            .size(px(30.))
                            .rounded_md()
                            .border_1()
                            .border_color(palette.border)
                            .bg(palette.elevated)
                            .hover(|button| button.bg(palette.hover))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(theme_icon(
                                if self.theme == ThemeMode::Dark {
                                    ThemeIcon::Moon
                                } else {
                                    ThemeIcon::Sun
                                },
                                palette.muted,
                            ))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_theme(cx))),
                    ),
            )
            .into_any_element()
    }
}
