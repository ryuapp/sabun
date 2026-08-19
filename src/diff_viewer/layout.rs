use super::row_geometry::cumulative_offsets;
use super::{
    DEFAULT_DIFF_FONT_SIZE, DiffDisplayRow, DiffFile, DiffLayout, DiffRowIndex,
    FILE_SCROLLBAR_WIDTH, MIN_DIFF_WIDTH, MIN_SIDEBAR_WIDTH, Palette, Pixels, Range,
    SCROLLBAR_WIDTH, SIDEBAR_RESIZE_HANDLE_WIDTH, ScrollHandle, TREE_SCROLLBAR_GAP, TextRun,
    Window, font, px, unpack_diff_row_index,
};
use unicode_width::UnicodeWidthChar;

const CODE_TAB_WIDTH: usize = 4;
const UNIFIED_GUTTER_WIDTH: f32 = 48.;
const SPLIT_GUTTER_WIDTH: f32 = 50.;
const UNIFIED_NON_GUTTER_WIDTH: f32 = 43.;
const SPLIT_NON_GUTTER_WIDTH: f32 = 40.;
const UNIFIED_CODE_INSET: f32 = 31.;
const SPLIT_CODE_INSET: f32 = 28.;

#[derive(Clone, Copy, Debug)]
enum DiffRowLayoutKind {
    Fixed,
    Scaled,
    Wrapped,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DiffRowLayout {
    value: u32,
    kind: DiffRowLayoutKind,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WrapWidthRange {
    minimum: Pixels,
    maximum: Option<Pixels>,
}

impl WrapWidthRange {
    const fn unconstrained() -> Self {
        Self {
            minimum: px(0.),
            maximum: None,
        }
    }

    pub(super) fn contains(self, width: Pixels) -> bool {
        width >= self.minimum && self.maximum.is_none_or(|maximum| width < maximum)
    }

    fn include(&mut self, minimum: Pixels, maximum: Option<Pixels>) {
        self.minimum = self.minimum.max(minimum);
        if let Some(maximum) = maximum {
            self.maximum = Some(self.maximum.map_or(maximum, |current| current.min(maximum)));
        }
    }
}

impl DiffRowLayout {
    fn fixed(height: Pixels) -> Self {
        Self {
            value: f32::from(height).to_bits(),
            kind: DiffRowLayoutKind::Fixed,
        }
    }

    fn scaled(height: Pixels) -> Self {
        Self {
            value: f32::from(height).to_bits(),
            kind: DiffRowLayoutKind::Scaled,
        }
    }

    fn wrapped(columns: usize) -> Self {
        Self {
            value: u32::try_from(columns).unwrap_or(u32::MAX),
            kind: DiffRowLayoutKind::Wrapped,
        }
    }

    const fn pixels(self) -> Pixels {
        px(f32::from_bits(self.value))
    }

    fn from_row(row: &DiffDisplayRow, files: &[DiffFile]) -> Self {
        match row {
            DiffDisplayRow::Split {
                file_index,
                hunk_index,
                old_line_index,
                new_line_index,
                ..
            } => Self::wrapped(
                [*old_line_index, *new_line_index]
                    .into_iter()
                    .filter_map(unpack_diff_row_index)
                    .filter_map(|line_index| {
                        files
                            .get(*file_index as usize)?
                            .hunks
                            .get(*hunk_index as usize)?
                            .lines
                            .get(line_index)
                    })
                    .map(|line| {
                        code_columns(
                            files[*file_index as usize].hunks[*hunk_index as usize]
                                .line_content(line),
                        )
                    })
                    .max()
                    .unwrap_or_default(),
            ),
            DiffDisplayRow::Unified {
                file_index,
                hunk_index,
                row_index,
                ..
            } => Self::wrapped(
                files
                    .get(*file_index as usize)
                    .and_then(|file| file.hunks.get(*hunk_index as usize))
                    .and_then(|hunk| hunk.lines.get(*row_index as usize))
                    .map_or(0, |line| {
                        code_columns(
                            files[*file_index as usize].hunks[*hunk_index as usize]
                                .line_content(line),
                        )
                    }),
            ),
            DiffDisplayRow::Separator { .. } => Self::scaled(row.height()),
            _ => Self::fixed(row.height()),
        }
    }
}

pub(super) fn diff_row_layouts(rows: &DiffRowIndex, files: &[DiffFile]) -> Vec<DiffRowLayout> {
    rows.iter()
        .map(|row| DiffRowLayout::from_row(&row, files))
        .collect()
}

pub(super) fn clamped_sidebar_width(width: Pixels, viewport_width: Pixels) -> Pixels {
    let maximum = (viewport_width - px(MIN_DIFF_WIDTH + SIDEBAR_RESIZE_HANDLE_WIDTH))
        .max(px(MIN_SIDEBAR_WIDTH));
    width.clamp(px(MIN_SIDEBAR_WIDTH), maximum)
}

pub(super) fn sidebar_file_name_width(sidebar_width: Pixels, depth: usize) -> Pixels {
    let indent = px((depth as f32).mul_add(14., 12.));
    (sidebar_width - indent - px(60. + FILE_SCROLLBAR_WIDTH + TREE_SCROLLBAR_GAP)).max(px(24.))
}

pub(super) fn wrapped_code_width(
    viewport_width: Pixels,
    sidebar_width: Pixels,
    layout: DiffLayout,
    font_size: Pixels,
) -> Pixels {
    let diff_width =
        (viewport_width - sidebar_width - px(SIDEBAR_RESIZE_HANDLE_WIDTH + SCROLLBAR_WIDTH))
            .max(px(240.));
    match layout {
        DiffLayout::Split => {
            (diff_width / 2. - diff_gutter_width(font_size, layout) - px(SPLIT_NON_GUTTER_WIDTH))
                .max(px(80.))
        }
        DiffLayout::Unified => {
            (diff_width - diff_gutter_width(font_size, layout) * 2. - px(UNIFIED_NON_GUTTER_WIDTH))
                .max(px(80.))
        }
    }
}

pub(super) fn diff_gutter_width(font_size: Pixels, layout: DiffLayout) -> Pixels {
    let default_width = match layout {
        DiffLayout::Split => SPLIT_GUTTER_WIDTH,
        DiffLayout::Unified => UNIFIED_GUTTER_WIDTH,
    };
    px(default_width) * (f32::from(font_size) / DEFAULT_DIFF_FONT_SIZE)
}

pub(super) fn diff_code_inset(font_size: Pixels, layout: DiffLayout) -> Pixels {
    let fixed_width = match layout {
        DiffLayout::Split => SPLIT_CODE_INSET,
        DiffLayout::Unified => UNIFIED_CODE_INSET,
    };
    let gutter_count = match layout {
        DiffLayout::Split => 1.,
        DiffLayout::Unified => 2.,
    };
    diff_gutter_width(font_size, layout) * gutter_count + px(fixed_width)
}

pub(super) fn wrapped_row_offsets(
    rows: &[DiffRowLayout],
    code_width: Pixels,
    font_size: Pixels,
    line_height: Pixels,
    padding_y: Pixels,
    window: &Window,
    palette: Palette,
) -> (Vec<Pixels>, WrapWidthRange) {
    let character_width = monospace_character_width(font_size, line_height, window, palette);
    let mut width_range = WrapWidthRange::unconstrained();
    let offsets = cumulative_offsets(rows, |row| match row.kind {
        DiffRowLayoutKind::Fixed => row.pixels(),
        DiffRowLayoutKind::Scaled => row.pixels() * (f32::from(font_size) / DEFAULT_DIFF_FONT_SIZE),
        DiffRowLayoutKind::Wrapped => wrapped_code_height(
            row.value as usize,
            code_width,
            line_height,
            character_width,
            padding_y,
            &mut width_range,
        ),
    });
    (offsets, width_range)
}

fn monospace_character_width(
    font_size: Pixels,
    line_height: Pixels,
    window: &Window,
    palette: Palette,
) -> Pixels {
    const SAMPLE: &str = "0";
    let runs = [TextRun {
        len: SAMPLE.len(),
        font: font("Cascadia Mono"),
        color: palette.text.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }];
    window
        .text_system()
        .shape_text(SAMPLE.into(), font_size, &runs, None, None)
        .ok()
        .and_then(|lines| lines.first().map(|line| line.size(line_height).width))
        .filter(|width| *width > px(0.))
        .unwrap_or(font_size * 0.6)
}

fn code_columns(content: &str) -> usize {
    content.chars().fold(0, |columns, character| {
        if character == '\t' {
            columns + CODE_TAB_WIDTH - columns % CODE_TAB_WIDTH
        } else {
            columns + UnicodeWidthChar::width(character).unwrap_or_default()
        }
    })
}

fn wrapped_code_height(
    columns: usize,
    code_width: Pixels,
    line_height: Pixels,
    character_width: Pixels,
    padding_y: Pixels,
    width_range: &mut WrapWidthRange,
) -> Pixels {
    if columns == 0 {
        return line_height + padding_y * 2.;
    }
    let content_width = character_width * columns as f32;
    let line_count = (f32::from(content_width) / f32::from(code_width.max(px(1.))))
        .ceil()
        .max(1.);
    let minimum = content_width / line_count;
    let maximum = (line_count > 1.).then(|| content_width / (line_count - 1.));
    width_range.include(minimum, maximum);
    line_height * line_count + padding_y * 2.
}

pub(super) fn variable_visible_range(
    offsets: &[Pixels],
    scroll_handle: &ScrollHandle,
    overdraw: f32,
) -> (Range<usize>, Pixels, Pixels) {
    let row_count = offsets.len().saturating_sub(1);
    if row_count == 0 {
        return (0..0, px(0.), px(0.));
    }
    let measured_viewport = scroll_handle.bounds().size.height;
    let viewport = if measured_viewport > px(0.) {
        measured_viewport
    } else {
        px(900.)
    };
    let scroll_top = (-scroll_handle.offset().y - px(overdraw)).max(px(0.));
    let scroll_bottom = -scroll_handle.offset().y + viewport + px(overdraw);
    let start = offsets
        .partition_point(|offset| *offset <= scroll_top)
        .saturating_sub(1)
        .min(row_count);
    let mut end = offsets
        .partition_point(|offset| *offset < scroll_bottom)
        .min(row_count);
    if end <= start {
        end = (start + 1).min(row_count);
    }
    let total = offsets[row_count];
    (start..end, offsets[start], total - offsets[end])
}

#[derive(Clone, Copy)]
pub(super) struct InterpolatedOffsets<'a> {
    base: &'a [Pixels],
    target: Option<&'a [Pixels]>,
    progress: f32,
}

impl<'a> InterpolatedOffsets<'a> {
    pub(super) fn new(base: &'a [Pixels], target: Option<&'a [Pixels]>, progress: f32) -> Self {
        Self {
            base,
            target: target.filter(|target| target.len() == base.len()),
            progress: progress.clamp(0., 1.),
        }
    }

    pub(super) fn get(self, index: usize) -> Option<Pixels> {
        let base = *self.base.get(index)?;
        Some(
            self.target
                .map_or(base, |target| base + (target[index] - base) * self.progress),
        )
    }

    pub(super) fn last(self) -> Option<Pixels> {
        self.get(self.base.len().checked_sub(1)?)
    }

    pub(super) fn row_height(self, index: usize) -> Pixels {
        self.get(index + 1).unwrap_or_default() - self.get(index).unwrap_or_default()
    }

    pub(super) fn row_index_at(self, position: Pixels, row_count: usize) -> usize {
        let mut left = 0;
        let mut right = self.base.len();
        while left < right {
            let middle = left + (right - left) / 2;
            if self.get(middle).is_some_and(|offset| offset <= position) {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        left.saturating_sub(1).min(row_count.saturating_sub(1))
    }

    pub(super) fn visible_range(
        self,
        scroll_handle: &ScrollHandle,
        overdraw: f32,
    ) -> (Range<usize>, Pixels, Pixels) {
        let row_count = self.base.len().saturating_sub(1);
        if row_count == 0 {
            return (0..0, px(0.), px(0.));
        }
        let measured_viewport = scroll_handle.bounds().size.height;
        let viewport = if measured_viewport > px(0.) {
            measured_viewport
        } else {
            px(900.)
        };
        let scroll_top = (-scroll_handle.offset().y - px(overdraw)).max(px(0.));
        let scroll_bottom = -scroll_handle.offset().y + viewport + px(overdraw);
        let start = self.row_index_at(scroll_top, row_count);
        let mut end = self.first_offset_at_least(scroll_bottom).min(row_count);
        if end <= start {
            end = (start + 1).min(row_count);
        }
        let top = self.get(start).unwrap_or_default();
        let bottom = self.last().unwrap_or_default() - self.get(end).unwrap_or_default();
        (start..end, top, bottom)
    }

    fn first_offset_at_least(self, position: Pixels) -> usize {
        let mut left = 0;
        let mut right = self.base.len();
        while left < right {
            let middle = left + (right - left) / 2;
            if self.get(middle).is_some_and(|offset| offset < position) {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        left
    }
}
