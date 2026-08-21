use super::{
    Context, DiffLayout, DiffStats, DiffViewer, FluentBuilder, FontWeight, InteractiveElement,
    IntoElement, LayoutToggle, Palette, ParentElement, StatefulInteractiveElement, Styled,
    ThemeIcon, ThemeMode, canvas, div, point, px, theme_icon,
};
use gpui::PathBuilder;

fn viewed_progress_gauge(progress: f32, palette: Palette) -> impl IntoElement {
    const SEGMENTS: usize = 32;
    let size = px(16.);
    canvas(
        move |_, _, _| progress.clamp(0., 1.),
        move |bounds, progress, window, _| {
            let paint_arc = |fraction: f32, color, window: &mut gpui::Window| {
                if fraction <= 0. {
                    return;
                }
                let mut path = PathBuilder::stroke(px(2.));
                for step in 0..=SEGMENTS {
                    let angle = std::f32::consts::TAU * fraction * step as f32 / SEGMENTS as f32
                        - std::f32::consts::FRAC_PI_2;
                    let position =
                        bounds.center() + point(px(angle.cos() * 6.), px(angle.sin() * 6.));
                    if step == 0 {
                        path.move_to(position);
                    } else {
                        path.line_to(position);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            };

            paint_arc(1., palette.border, window);
            paint_arc(progress, palette.green, window);
        },
    )
    .size(size)
}

impl DiffViewer {
    pub(super) fn render_top_bar(
        &mut self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let total_additions = self.total_additions;
        let total_deletions = self.total_deletions;
        let file_count = self.diff.files.len();
        let viewed_count = self.viewed_files.len().min(file_count);
        let viewed_progress = if file_count == 0 {
            0.
        } else {
            viewed_count as f32 / file_count as f32
        };
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
                    .child(
                        div()
                            .id("viewed-progress")
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(viewed_progress_gauge(viewed_progress, palette))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if viewed_count == 0 {
                                        palette.muted
                                    } else {
                                        palette.green
                                    })
                                    .child(format!("{viewed_count}/{file_count} viewed")),
                            ),
                    )
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
