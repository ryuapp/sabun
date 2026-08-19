use std::ops::Range;

use super::{
    Arc, DiffDisplayRow, DiffViewer, HighlightLines, HighlightStyle, IntoElement, LineKind,
    Palette, Pixels, Rgba, SharedString, Side, Styled, StyledText, TextRun, ThemeMode, canvas,
    combine_highlights, fill, font, inline_ranges, point, px, size, syntax_highlighter,
    syntax_highlights, syntax_highlights_with_state, unpack_diff_row_index,
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct SyntaxCacheKey {
    pub(super) theme: ThemeMode,
    pub(super) display_index: usize,
    pub(super) side: Side,
    pub(super) language: &'static str,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct InlineCacheKey {
    pub(super) display_index: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct SyntaxPosition {
    pub(super) file_index: usize,
    pub(super) hunk_index: usize,
    pub(super) line_number: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct SyntaxStreamKey {
    theme: ThemeMode,
    file_index: usize,
    hunk_index: usize,
    side: Side,
    language: &'static str,
}

pub(super) struct SyntaxStreamState {
    highlighter: HighlightLines<'static>,
    embedded_highlighter: Option<(&'static str, HighlightLines<'static>)>,
    line_buffer: String,
    next_line_number: Option<u32>,
}

impl SyntaxStreamState {
    fn new(language: &'static str, theme: ThemeMode) -> Self {
        Self {
            highlighter: syntax_highlighter(language, theme),
            embedded_highlighter: None,
            line_buffer: String::new(),
            next_line_number: None,
        }
    }

    fn highlights(
        &mut self,
        line_number: u32,
        content: &str,
        language: &'static str,
        theme: ThemeMode,
        palette: Palette,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        if self.next_line_number != Some(line_number) {
            *self = Self::new(language, theme);
        }
        let highlights = syntax_highlights_with_state(
            &mut self.highlighter,
            &mut self.embedded_highlighter,
            &mut self.line_buffer,
            content,
            language,
            palette,
        );
        self.next_line_number = line_number.checked_add(1);
        highlights
    }
}

#[derive(Clone)]
pub(super) struct CachedCodeLine {
    pub(super) text: SharedString,
    pub(super) highlights: Vec<(Range<usize>, HighlightStyle)>,
    kind: Option<LineKind>,
    inline: InlineHighlightRanges,
}

type InlineHighlightRanges = Arc<[Range<usize>]>;
pub(super) type InlineHighlightPair = (InlineHighlightRanges, InlineHighlightRanges);
const MAX_SYNTAX_STREAMS: usize = 1_024;

#[derive(Clone)]
pub(super) struct CachedInlineHighlightPair {
    old: Arc<str>,
    new: Arc<str>,
    ranges: InlineHighlightPair,
}

impl DiffViewer {
    pub(super) fn retain_valid_syntax_cache(&mut self) {
        self.syntax_streams.clear();
        let rows = &self.diff_rows;
        let file_meta = &self.file_meta;
        self.syntax_cache.retain(|key, cached| {
            let (content, file_index, expected_side) = match rows.get(key.display_index) {
                Some(DiffDisplayRow::Split {
                    file_index,
                    hunk_index,
                    old_line_index,
                    new_line_index,
                    ..
                }) => {
                    let line_index = match key.side {
                        Side::Old => old_line_index,
                        Side::New => new_line_index,
                    };
                    unpack_diff_row_index(line_index).and_then(|line_index| {
                        let hunk = self
                            .diff
                            .files
                            .get(file_index as usize)?
                            .hunks
                            .get(hunk_index as usize)?;
                        hunk.lines
                            .get(line_index)
                            .map(|line| (hunk.line_content(line), file_index as usize, key.side))
                    })
                }
                Some(DiffDisplayRow::Unified {
                    file_index,
                    hunk_index,
                    row_index,
                    ..
                }) => self.diff.files.get(file_index as usize).and_then(|file| {
                    let hunk = file.hunks.get(hunk_index as usize)?;
                    hunk.lines.get(row_index as usize).map(|line| {
                        (
                            hunk.line_content(line),
                            file_index as usize,
                            if line.kind == LineKind::Deletion {
                                Side::Old
                            } else {
                                Side::New
                            },
                        )
                    })
                }),
                _ => None,
            }
            .unwrap_or(("", usize::MAX, key.side));
            let language_matches = file_meta
                .get(file_index)
                .is_some_and(|file| file.language == key.language);
            expected_side == key.side && language_matches && content == cached.text.as_ref()
        });
    }

    pub(super) fn inline_pair(
        &mut self,
        display_index: usize,
        old: &str,
        new: &str,
    ) -> InlineHighlightPair {
        let key = InlineCacheKey { display_index };
        if let Some(cached) = self.inline_cache.get(&key)
            && cached.old.as_ref() == old
            && cached.new.as_ref() == new
        {
            return cached.ranges.clone();
        }
        if self.inline_cache.len() >= 32_768 {
            self.inline_cache.clear();
        }
        let (old_ranges, new_ranges) = inline_ranges(old, new);
        let ranges = (Arc::from(old_ranges), Arc::from(new_ranges));
        self.inline_cache.insert(
            key,
            CachedInlineHighlightPair {
                old: Arc::from(old),
                new: Arc::from(new),
                ranges: ranges.clone(),
            },
        );
        ranges
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn code_text(
        &mut self,
        display_index: usize,
        side: Side,
        content: &str,
        language: &'static str,
        inline: Option<InlineHighlightRanges>,
        kind: Option<LineKind>,
        selection: Option<Range<usize>>,
        palette: Palette,
        syntax_position: Option<SyntaxPosition>,
    ) -> gpui::AnyElement {
        let key = SyntaxCacheKey {
            theme: self.theme,
            display_index,
            side,
            language,
        };
        let inline = inline.unwrap_or_default();
        let (text, highlights) = if let Some(line) = self
            .syntax_cache
            .get(&key)
            .filter(|line| line.kind == kind && line.inline == inline && line.text == content)
        {
            (line.text.clone(), line.highlights.clone())
        } else {
            if self.syntax_cache.len() >= 65_536 {
                self.syntax_cache.clear();
            }
            let text = SharedString::from(content.to_owned());
            let inline_background = match kind {
                Some(LineKind::Deletion) => palette.red_inline,
                _ => palette.green_inline,
            };
            let syntax_highlights = syntax_position.map_or_else(
                || syntax_highlights(&text, language, palette),
                |position| {
                    self.stateful_syntax_highlights(position, side, &text, language, palette)
                },
            );
            let highlights = combine_highlights(
                syntax_highlights,
                inline
                    .iter()
                    .filter(|range| !range.is_empty())
                    .cloned()
                    .map(|range| {
                        (
                            range,
                            HighlightStyle {
                                background_color: Some(inline_background.into()),
                                ..Default::default()
                            },
                        )
                    }),
            )
            .collect::<Vec<_>>();
            let line = CachedCodeLine {
                text,
                highlights,
                kind,
                inline,
            };
            let result = (line.text.clone(), line.highlights.clone());
            self.syntax_cache.insert(key, line);
            result
        };
        let highlights = if let Some(range) = selection.filter(|range| !range.is_empty()) {
            apply_selection_background(highlights, range, palette.selection)
        } else {
            highlights
        };
        StyledText::new(text)
            .with_highlights(highlights)
            .into_any_element()
    }

    fn stateful_syntax_highlights(
        &mut self,
        position: SyntaxPosition,
        side: Side,
        content: &str,
        language: &'static str,
        palette: Palette,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        if self.syntax_streams.len() >= MAX_SYNTAX_STREAMS {
            self.syntax_streams.clear();
        }
        let key = SyntaxStreamKey {
            theme: self.theme,
            file_index: position.file_index,
            hunk_index: position.hunk_index,
            side,
            language,
        };
        self.syntax_streams
            .entry(key)
            .or_insert_with(|| SyntaxStreamState::new(language, self.theme))
            .highlights(position.line_number, content, language, self.theme, palette)
    }
}

pub(super) const fn selection_padding_edges(
    selection: &Range<usize>,
    text_len: usize,
) -> (bool, bool) {
    (selection.start == 0, selection.end == text_len)
}

pub(super) fn selection_padding_overlay(
    content: &str,
    selection: Range<usize>,
    selection_color: Rgba,
    font_size: Pixels,
    line_height: Pixels,
    padding: Pixels,
) -> Option<gpui::AnyElement> {
    if selection.is_empty() {
        return None;
    }
    let (paint_top, paint_bottom) = selection_padding_edges(&selection, content.len());
    if !paint_top && !paint_bottom {
        return None;
    }

    let text = SharedString::from(content.to_owned());
    let text_len = text.len();
    Some(
        canvas(
            move |bounds, window, _| {
                let runs = [TextRun {
                    len: text_len,
                    font: font("Cascadia Mono"),
                    color: selection_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }];
                window
                    .text_system()
                    .shape_text(text, font_size, &runs, Some(bounds.size.width), None)
                    .ok()
                    .and_then(|mut lines| lines.pop())
            },
            move |bounds, shaped, window, _| {
                let Some(line) = shaped else {
                    return;
                };
                let line_ranges = line
                    .wrap_boundaries()
                    .iter()
                    .map(|boundary| line.runs()[boundary.run_ix].glyphs[boundary.glyph_ix].index)
                    .chain([line.len()])
                    .scan(0, |start, end| {
                        let range = *start..end;
                        *start = end;
                        Some(range)
                    })
                    .collect::<Vec<_>>();

                if paint_top
                    && let Some(first) = line_ranges.first()
                    && let Some(end) =
                        line.position_for_index(selection.end.min(first.end), line_height)
                    && end.x > px(0.)
                {
                    window.paint_quad(fill(
                        gpui::Bounds::new(bounds.origin, size(end.x, padding)),
                        selection_color,
                    ));
                }

                if paint_bottom && let Some(last) = line_ranges.last() {
                    let start_index = selection.start.max(last.start);
                    let start = line
                        .position_for_index(start_index, line_height)
                        .unwrap_or_else(|| point(px(0.), px(0.)));
                    let end = line
                        .position_for_index(last.end, line_height)
                        .unwrap_or_else(|| point(line.width(), px(0.)));
                    if end.x > start.x {
                        window.paint_quad(fill(
                            gpui::Bounds::new(
                                point(bounds.left() + start.x, bounds.bottom() - padding),
                                size(end.x - start.x, padding),
                            ),
                            selection_color,
                        ));
                    }
                }
            },
        )
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .into_any_element(),
    )
}

pub(super) fn apply_selection_background(
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    selection: Range<usize>,
    selection_color: Rgba,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let selection_range = selection.clone();
    combine_highlights(
        highlights,
        [(
            selection,
            HighlightStyle {
                background_color: Some(selection_color.into()),
                ..Default::default()
            },
        )],
    )
    .map(|(range, mut style)| {
        if range.start < selection_range.end && selection_range.start < range.end {
            style.background_color = Some(selection_color.into());
        }
        (range, style)
    })
    .collect()
}
