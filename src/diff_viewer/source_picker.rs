use super::{
    Context, DiffViewer, FluentBuilder, FontWeight, InteractiveElement, IntoElement, Palette,
    ParentElement, SOURCE_PICKER_SCROLLBAR_WIDTH, ScrollbarAxis, ScrollbarTarget,
    SourceCatalogLoadState, SourcePickerSection, StatefulInteractiveElement, Styled, div, point,
    px, relative, with_alpha,
};
use crate::cli::{
    GitCommitSource, GitDiffSource, GitDiffSourceSwitcher, GitSourceCatalog, GitStashSource,
};

const HISTORY_LOAD_THRESHOLD: f32 = 180.;

impl DiffViewer {
    pub(super) fn toggle_source_picker(&mut self, cx: &mut Context<Self>) {
        if self.source_picker_open {
            self.close_source_picker(cx);
            return;
        }
        let Some(source_switcher) = self.source_switcher.clone() else {
            return;
        };
        self.source_picker_section = match source_switcher.source() {
            GitDiffSource::Changes | GitDiffSource::Staged(_) => SourcePickerSection::Changes,
            GitDiffSource::Comparison(_) | GitDiffSource::Commit(_) => SourcePickerSection::History,
            GitDiffSource::Stash(_) => SourcePickerSection::Stashes,
        };
        self.text_context_menu = None;
        self.path_context_menu = None;
        self.source_error = None;
        self.source_catalog = None;
        self.source_catalog_load_state = SourceCatalogLoadState::Initial;
        self.source_catalog_generation = self.source_catalog_generation.wrapping_add(1);
        let generation = self.source_catalog_generation;
        self.source_picker_scroll.set_offset(point(px(0.), px(0.)));
        self.source_picker_open = true;
        cx.notify();

        cx.spawn(async move |viewer, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { source_switcher.catalog() })
                .await;
            let _ = viewer.update(cx, |viewer, cx| {
                if !viewer.source_picker_open || viewer.source_catalog_generation != generation {
                    return;
                }
                viewer.source_catalog_load_state = SourceCatalogLoadState::Idle;
                match result {
                    Ok(catalog) => viewer.source_catalog = Some(catalog),
                    Err(error) => viewer.source_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn close_source_picker(&mut self, cx: &mut Context<Self>) {
        self.source_picker_open = false;
        self.source_catalog = None;
        self.source_catalog_load_state = SourceCatalogLoadState::Idle;
        self.source_catalog_generation = self.source_catalog_generation.wrapping_add(1);
        self.source_error = None;
        cx.notify();
    }

    fn switch_source(&mut self, source: GitDiffSource, cx: &mut Context<Self>) {
        let Some(source_switcher) = self.source_switcher.clone() else {
            return;
        };
        if source_switcher.source() == source {
            self.close_source_picker(cx);
            return;
        }
        match source_switcher.switch_to(source) {
            Ok(input) => {
                self.source_picker_open = false;
                self.source_catalog = None;
                self.apply_input(input, cx);
            }
            Err(error) => {
                self.source_picker_open = true;
                self.source_error = Some(error);
                cx.notify();
            }
        }
    }

    pub(super) fn render_source_picker(
        &self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let current = self
            .source_switcher
            .as_ref()
            .map(GitDiffSourceSwitcher::source);
        let GitSourceCatalog {
            commits,
            has_more_commits,
            stashes,
        } = self
            .source_catalog
            .clone()
            .unwrap_or_else(|| GitSourceCatalog {
                commits: Vec::new(),
                has_more_commits: false,
                stashes: Vec::new(),
            });
        let initial_loading = self.source_catalog_load_state == SourceCatalogLoadState::Initial;

        div()
            .id("source-picker-backdrop")
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .flex()
            .items_start()
            .justify_center()
            .bg(with_alpha(palette.canvas, 0.78))
            .occlude()
            .on_click(cx.listener(|this, _, _, cx| this.close_source_picker(cx)))
            .child(
                div()
                    .id("source-picker")
                    .mt(px(48.))
                    .w(relative(0.86))
                    .max_w(px(820.))
                    .h(relative(0.78))
                    .max_h(px(640.))
                    .rounded_md()
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.elevated)
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                    .child(
                        div()
                            .h(px(64.))
                            .px_4()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(palette.border)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(palette.text)
                                            .child("Choose what to view"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(palette.muted)
                                            .child("Working changes, commits, and stashes"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("close-source-picker")
                                    .size(px(32.))
                                    .rounded_md()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(17.))
                                    .text_color(palette.muted)
                                    .hover(|button| button.bg(palette.hover))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_source_picker(cx);
                                    }))
                                    .child("✕"),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .child(self.render_source_sections(
                                commits.len(),
                                has_more_commits,
                                stashes.len(),
                                initial_loading,
                                palette,
                                cx,
                            ))
                            .child(
                                div()
                                    .id("source-picker-details")
                                    .relative()
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .id("source-picker-details-scroll")
                                            .size_full()
                                            .overflow_y_scroll()
                                            .track_scroll(&self.source_picker_scroll)
                                            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                                                this.maybe_load_more_history(cx);
                                            }))
                                            .p_4()
                                            .pr(px(16. + SOURCE_PICKER_SCROLLBAR_WIDTH))
                                            .child(match self.source_picker_section {
                                                SourcePickerSection::Changes => self
                                                    .render_working_sources(
                                                        current.as_ref(),
                                                        palette,
                                                        cx,
                                                    ),
                                                SourcePickerSection::History => self
                                                    .render_commit_sources(
                                                        &commits,
                                                        has_more_commits,
                                                        current.as_ref(),
                                                        palette,
                                                        cx,
                                                    ),
                                                SourcePickerSection::Stashes => self
                                                    .render_stash_sources(
                                                        &stashes,
                                                        initial_loading,
                                                        current.as_ref(),
                                                        palette,
                                                        cx,
                                                    ),
                                            })
                                            .children(self.source_error.as_ref().map(|error| {
                                                div()
                                                    .mt_3()
                                                    .px_3()
                                                    .py_2()
                                                    .rounded_md()
                                                    .bg(palette.red_bg)
                                                    .text_xs()
                                                    .text_color(palette.red)
                                                    .child(error.clone())
                                            })),
                                    )
                                    .child(self.render_scrollbar(
                                        ScrollbarTarget::SourcePicker,
                                        ScrollbarAxis::Vertical,
                                        self.source_picker_scroll.clone(),
                                        palette,
                                        cx,
                                    )),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_source_sections(
        &self,
        commit_count: usize,
        has_more_commits: bool,
        stash_count: usize,
        initial_loading: bool,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .w(px(190.))
            .h_full()
            .flex_none()
            .p_2()
            .border_r_1()
            .border_color(palette.border)
            .bg(palette.sidebar)
            .flex()
            .flex_col()
            .gap_1()
            .child(category_row(
                "source-category-changes",
                "Changes",
                None,
                self.source_picker_section == SourcePickerSection::Changes,
                palette,
                cx.listener(|this, _, _, cx| {
                    this.select_source_picker_section(SourcePickerSection::Changes, cx);
                }),
            ))
            .child(category_row(
                "source-category-history",
                "History",
                Some(if initial_loading {
                    "…".to_owned()
                } else if has_more_commits {
                    format!("{commit_count}+")
                } else {
                    commit_count.to_string()
                }),
                self.source_picker_section == SourcePickerSection::History,
                palette,
                cx.listener(|this, _, _, cx| {
                    this.select_source_picker_section(SourcePickerSection::History, cx);
                }),
            ))
            .child(category_row(
                "source-category-stashes",
                "Stashes",
                Some(if initial_loading {
                    "…".to_owned()
                } else {
                    stash_count.to_string()
                }),
                self.source_picker_section == SourcePickerSection::Stashes,
                palette,
                cx.listener(|this, _, _, cx| {
                    this.select_source_picker_section(SourcePickerSection::Stashes, cx);
                }),
            ))
            .into_any_element()
    }

    fn select_source_picker_section(
        &mut self,
        section: SourcePickerSection,
        cx: &mut Context<Self>,
    ) {
        if self.source_picker_section != section {
            self.source_picker_section = section;
            self.source_picker_scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
        }
    }

    fn render_working_sources(
        &self,
        current: Option<&GitDiffSource>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(section_label("Changes", palette))
            .child(source_row(
                "source-picker-changes",
                "Changes",
                "All staged, unstaged, and untracked changes",
                current.is_some_and(|source| matches!(source, GitDiffSource::Changes)),
                palette,
                cx.listener(|this, _, _, cx| {
                    this.switch_source(GitDiffSource::Changes, cx);
                }),
            ))
            .child(source_row(
                "source-picker-staged",
                "Staged changes",
                "Changes currently in the Git index",
                current.is_some_and(|source| matches!(source, GitDiffSource::Staged(_))),
                palette,
                cx.listener(|this, _, _, cx| {
                    this.switch_source(GitDiffSource::Staged(None), cx);
                }),
            ))
            .into_any_element()
    }

    fn render_commit_sources(
        &self,
        commits: &[GitCommitSource],
        has_more: bool,
        current: Option<&GitDiffSource>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let rows = commits.iter().enumerate().map(|(index, commit)| {
            let target = commit.oid.clone();
            let active = current.is_some_and(
                |source| matches!(source, GitDiffSource::Commit(Some(value)) if value == &target),
            );
            source_row(
                ("source-picker-commit", index),
                commit.summary.clone(),
                format!("{} · {}", commit.short_oid, commit.author),
                active,
                palette,
                cx.listener(move |this, _, _, cx| {
                    this.switch_source(GitDiffSource::Commit(Some(target.clone())), cx);
                }),
            )
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(section_label("History", palette))
            .children(rows)
            .when(
                self.source_catalog_load_state == SourceCatalogLoadState::Initial,
                |section| section.child(source_status("Loading history…", palette)),
            )
            .when(
                self.source_catalog_load_state == SourceCatalogLoadState::MoreHistory,
                |section| section.child(source_status("Loading more…", palette)),
            )
            .when(
                self.source_catalog_load_state == SourceCatalogLoadState::Idle
                    && !has_more
                    && !commits.is_empty()
                    && self.source_error.is_none(),
                |section| section.child(source_status("End of history", palette)),
            )
            .when(
                self.source_catalog_load_state == SourceCatalogLoadState::Idle
                    && commits.is_empty()
                    && self.source_error.is_none(),
                |section| section.child(empty_section("No commits found", palette)),
            )
            .into_any_element()
    }

    pub(super) fn maybe_load_more_history(&mut self, cx: &mut Context<Self>) {
        if !self.source_picker_open
            || self.source_picker_section != SourcePickerSection::History
            || self.source_catalog_load_state != SourceCatalogLoadState::Idle
        {
            return;
        }
        let remaining =
            self.source_picker_scroll.max_offset().height + self.source_picker_scroll.offset().y;
        if remaining > px(HISTORY_LOAD_THRESHOLD) {
            return;
        }
        let Some(catalog) = self.source_catalog.as_ref() else {
            return;
        };
        if !catalog.has_more_commits {
            return;
        }
        let offset = catalog.commits.len();
        let Some(source_switcher) = self.source_switcher.clone() else {
            return;
        };
        let generation = self.source_catalog_generation;
        self.source_catalog_load_state = SourceCatalogLoadState::MoreHistory;
        self.source_error = None;
        cx.notify();

        cx.spawn(async move |viewer, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { source_switcher.commit_page(offset) })
                .await;
            let _ = viewer.update(cx, |viewer, cx| {
                if !viewer.source_picker_open || viewer.source_catalog_generation != generation {
                    return;
                }
                viewer.source_catalog_load_state = SourceCatalogLoadState::Idle;
                match result {
                    Ok(page) => {
                        if let Some(catalog) = viewer.source_catalog.as_mut()
                            && catalog.commits.len() == offset
                        {
                            catalog.commits.extend(page.commits);
                            catalog.has_more_commits = page.has_more;
                        }
                    }
                    Err(error) => viewer.source_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_stash_sources(
        &self,
        stashes: &[GitStashSource],
        initial_loading: bool,
        current: Option<&GitDiffSource>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let rows = stashes.iter().enumerate().map(|(index, stash)| {
            let reference = stash.reference.clone();
            let active = current.is_some_and(
                |source| matches!(source, GitDiffSource::Stash(Some(value)) if value == &reference),
            );
            source_row(
                ("source-picker-stash", index),
                stash.summary.clone(),
                format!("{} · {}", stash.reference, stash.short_oid),
                active,
                palette,
                cx.listener(move |this, _, _, cx| {
                    this.switch_source(GitDiffSource::Stash(Some(reference.clone())), cx);
                }),
            )
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(section_label("Stashes", palette))
            .children(rows)
            .when(initial_loading, |section| {
                section.child(source_status("Loading stashes…", palette))
            })
            .when(
                !initial_loading && stashes.is_empty() && self.source_error.is_none(),
                |section| section.child(empty_section("No stashes found", palette)),
            )
            .into_any_element()
    }
}

fn section_label(label: &'static str, palette: Palette) -> gpui::AnyElement {
    div()
        .px_2()
        .pt_1()
        .pb_2()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette.text)
        .child(label)
        .into_any_element()
}

fn category_row(
    id: &'static str,
    label: &'static str,
    count: Option<String>,
    active: bool,
    palette: Palette,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .h(px(40.))
        .px_3()
        .rounded_md()
        .flex()
        .items_center()
        .justify_between()
        .text_sm()
        .text_color(if active { palette.text } else { palette.muted })
        .when(active, |row| row.bg(palette.selection))
        .hover(|row| row.bg(palette.hover))
        .on_click(on_click)
        .child(label)
        .children(count.map(|count| div().text_xs().text_color(palette.faint).child(count)))
        .into_any_element()
}

fn empty_section(label: &'static str, palette: Palette) -> gpui::AnyElement {
    div()
        .px_3()
        .py_2()
        .text_xs()
        .text_color(palette.faint)
        .child(label)
        .into_any_element()
}

fn source_status(label: &'static str, palette: Palette) -> gpui::AnyElement {
    div()
        .h(px(40.))
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(palette.faint)
        .child(label)
        .into_any_element()
}

fn source_row(
    id: impl Into<gpui::ElementId>,
    title: impl Into<gpui::SharedString>,
    detail: impl Into<gpui::SharedString>,
    active: bool,
    palette: Palette,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let title: gpui::SharedString = title.into();
    let detail: gpui::SharedString = detail.into();
    div()
        .id(id)
        .min_h(px(48.))
        .px_3()
        .py_2()
        .rounded_md()
        .flex()
        .items_center()
        .gap_3()
        .when(active, |row| row.bg(palette.selection))
        .hover(|row| row.bg(palette.hover))
        .on_click(on_click)
        .child(
            div()
                .w(px(12.))
                .flex_none()
                .text_color(palette.green)
                .child(if active { "●" } else { "" }),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().text_color(palette.text).child(title))
                .child(div().text_xs().text_color(palette.muted).child(detail)),
        )
        .into_any_element()
}
