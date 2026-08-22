use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use crate::cli::{GitDiffSourceSwitcher, GitSourceCatalog, Input, Launch, WatchRequest};
use crate::diff::{DiffFile, DiffHunk, DiffLine, DiffSet, LineKind};
use crate::icons::{ExpandIconDirection, ThemeIcon, context_expand_icon, theme_icon};
use gpui::prelude::FluentBuilder;
use gpui::{
    Animation, AnimationExt, AnyElement, App, AppContext, Application, Bounds, ClickEvent,
    ClipboardItem, Context, CursorStyle, FocusHandle, FontStyle, FontWeight, HighlightStyle,
    InteractiveElement, IntoElement, IsZero, KeyBinding, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, RenderOnce, Rgba,
    ScrollDelta, ScrollHandle, ScrollWheelEvent, SharedString, Size, StatefulInteractiveElement,
    Styled, StyledText, Subscription, TextRun, TitlebarOptions, Window, WindowBounds,
    WindowOptions, actions, canvas, combine_highlights, div, ease_out_quint, fill, font, point, px,
    relative, rgb, size,
};
use similar::{
    Algorithm, ChangeTag,
    utils::{diff_graphemes, diff_unicode_words},
};
use syntect::{
    easy::HighlightLines, highlighting::FontStyle as SyntectFontStyle, parsing::SyntaxSet,
};

mod code_text;
mod components;
mod context_menu;
mod diff_body;
mod diff_rows;
mod file_header;
mod inline_diff;
mod layout;
mod navigation;
mod row_geometry;
mod scroll;
mod scrollbar;
mod sidebar;
mod source_picker;
mod split_view;
mod syntax;
#[cfg(test)]
mod tests;
mod text_selection;
mod theme;
mod top_bar;
mod tree_layout;
mod unified_view;
mod watch;
mod zoom;

use code_text::{
    CachedCodeLine, CachedInlineHighlightPair, InlineCacheKey, SyntaxCacheKey, SyntaxPosition,
    SyntaxStreamKey, SyntaxStreamState,
};
#[cfg(test)]
use code_text::{apply_selection_background, selection_padding_edges};
use components::{
    ContextSeparator, DiffChangeBar, DiffCodeCell, DiffGutter, DiffLineAppearance, DiffMarker,
    DiffStats, EmptyState, FileChangeBadge, LayoutToggle, ScrollViewport, VirtualizedColumn,
};
#[cfg(test)]
use context_menu::clamp_context_menu_position;
use context_menu::{CopyPathFeedback, CopyPathFeedbackPhase, PathContextMenu};
use diff_rows::{DiffRowData, DiffRowIndex, build_diff_row_data, collapse_file_rows};
#[cfg(test)]
use diff_rows::{build_diff_rows, build_file_diff_rows, paired_line, row_offsets};
use inline_diff::inline_ranges;
use layout::{
    DiffRowLayout, InterpolatedOffsets, WrapWidthRange, clamped_sidebar_width, diff_code_inset,
    diff_gutter_width, diff_row_layouts, sidebar_file_name_width, variable_visible_range,
    wrapped_code_width,
};
#[cfg(test)]
use scroll::{
    WHEEL_PIXELS_PER_LINE, accumulate_scroll_target, middle_auto_scroll_velocity,
    wheel_zoom_direction, windows_vertical_pan_cursor_id,
};
use scroll::{
    scrollbar_axis_length, scrollbar_axis_position, scrollbar_axis_start, scrollbar_metrics,
    set_scrollbar_offset, vertical_auto_scroll_cursor,
};
#[cfg(test)]
use syntax::{detected_language, syntax_set, syntax_theme};
use syntax::{
    detected_language_for_file, language_color, syntax_highlighter, syntax_highlights,
    syntax_highlights_with_state, trim_diff_prefix,
};
use text_selection::{HeaderTextSelection, TextLane, TextSelection};
use theme::{Palette, ThemeMode, with_alpha};
use tree_layout::{
    FileTreeData, build_file_tree_data, normalized_path_components, sticky_file_tree_directories,
};
#[cfg(test)]
use tree_layout::{build_file_tree_rows, file_tree_row_offsets};
use zoom::DiffZoomAnchor;

actions!(
    sabun,
    [
        ToggleLayout,
        ToggleTheme,
        ToggleViewed,
        NextFile,
        PreviousFile,
        CopyTextSelection
    ]
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiffLayout {
    Split,
    Unified,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SourcePickerSection {
    #[default]
    Changes,
    History,
    Stashes,
}

const DEFAULT_DIFF_LAYOUT: DiffLayout = DiffLayout::Unified;
const SCROLLBAR_WIDTH: f32 = 16.;
const FILE_SCROLLBAR_WIDTH: f32 = SCROLLBAR_WIDTH / 2.;
const SOURCE_PICKER_SCROLLBAR_WIDTH: f32 = 10.;
const TREE_SCROLLBAR_GAP: f32 = 2.;
const TREE_DIRECTORY_ROW_HEIGHT: f32 = 28.;
const TREE_FILE_ROW_HEIGHT: f32 = 28.;
const DIFF_ROW_HEIGHT: f32 = 21.;
const DIFF_CODE_LINE_HEIGHT: f32 = 17.;
const DIFF_CODE_PADDING_Y: f32 = 2.;
const DEFAULT_DIFF_FONT_SIZE: f32 = 12.;
const DEFAULT_DIFF_GUTTER_FONT_SIZE: f32 = 10.;
const DEFAULT_SIDEBAR_WIDTH: f32 = 268.;
const MIN_SIDEBAR_WIDTH: f32 = 180.;
const MIN_DIFF_WIDTH: f32 = 360.;
const SIDEBAR_RESIZE_HANDLE_WIDTH: f32 = 5.;
const MIN_WINDOW_WIDTH: f32 = 640.;
const MIN_WINDOW_HEIGHT: f32 = 480.;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Side {
    Old,
    New,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollbarAxis {
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollbarTarget {
    Files,
    DiffVertical,
    SourcePicker,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SourceCatalogLoadState {
    #[default]
    Idle,
    Initial,
    MoreHistory,
}

impl ScrollbarTarget {
    const fn smooth_target(self) -> Option<SmoothScrollTarget> {
        match self {
            Self::Files => Some(SmoothScrollTarget::Files),
            Self::DiffVertical => Some(SmoothScrollTarget::Diff),
            Self::SourcePicker => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SmoothScrollTarget {
    Files,
    Diff,
}

impl SmoothScrollTarget {
    const fn scrollbar_target(self) -> ScrollbarTarget {
        match self {
            Self::Files => ScrollbarTarget::Files,
            Self::Diff => ScrollbarTarget::DiffVertical,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SmoothScrollState {
    target: Point<Pixels>,
    running: bool,
    last_frame: Option<Instant>,
}

impl SmoothScrollState {
    fn reset(&mut self) {
        *self = Self::default();
    }

    const fn stop_at(&mut self, target: Point<Pixels>) {
        self.target = target;
        self.running = false;
        self.last_frame = None;
    }
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarDrag {
    target: ScrollbarTarget,
    inside_thumb: Pixels,
}

#[derive(Clone, Copy, Debug)]
struct SidebarResizeDrag {
    pointer_x: Pixels,
    width: Pixels,
}

#[derive(Clone, Copy, Debug)]
struct MiddleAutoScroll {
    target: SmoothScrollTarget,
    anchor: Point<Pixels>,
    cursor: Point<Pixels>,
    last_frame: Instant,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ContextGap {
    file_index: usize,
    hunk_index: usize,
    start: usize,
    end: usize,
    position: ContextGapPosition,
    source: ContextGapSource,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ContextGapSource {
    Hunk,
    File { old_start: u32, new_start: u32 },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ContextGapPosition {
    Leading,
    Middle,
    Trailing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextExpandDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ContextExpansion {
    from_start: usize,
    from_end: usize,
}

impl ContextExpansion {
    fn reveal(&mut self, gap: ContextGap, direction: ContextExpandDirection) -> bool {
        let remaining = (gap.end - gap.start)
            .saturating_sub(self.from_start)
            .saturating_sub(self.from_end);
        if remaining == 0 {
            return false;
        }
        let reveal = remaining.min(CONTEXT_LINES_PER_STEP);
        match direction {
            ContextExpandDirection::Up => self.from_end += reveal,
            ContextExpandDirection::Down => self.from_start += reveal,
        }
        true
    }
}

const CONTEXT_LINES_PER_STEP: usize = 30;

const NO_DIFF_ROW_INDEX: u32 = u32::MAX;

fn pack_diff_row_index(index: usize) -> u32 {
    let index = u32::try_from(index).expect("diff row index exceeds u32");
    assert_ne!(index, NO_DIFF_ROW_INDEX, "diff row index uses sentinel");
    index
}

const fn unpack_diff_row_index(index: u32) -> Option<usize> {
    if index == NO_DIFF_ROW_INDEX {
        None
    } else {
        Some(index as usize)
    }
}

#[derive(Clone, Copy, Debug)]
enum DiffDisplayRow {
    FileGap,
    FileHeader {
        file_index: u32,
    },
    Separator {
        file_index: u32,
        hidden: u32,
        gap_index: u32,
        position: Option<ContextGapPosition>,
    },
    Split {
        file_index: u32,
        hunk_index: u32,
        old_line_index: u32,
        new_line_index: u32,
    },
    Unified {
        file_index: u32,
        hunk_index: u32,
        row_index: u32,
        counterpart_index: u32,
    },
    Empty {
        file_index: u32,
    },
}

impl DiffDisplayRow {
    const fn height(&self) -> Pixels {
        match self {
            Self::FileGap => px(18.),
            Self::FileHeader { .. } => px(44.),
            Self::Separator {
                position: Some(ContextGapPosition::Middle),
                ..
            } => px(DIFF_ROW_HEIGHT * 2.),
            Self::Separator { .. } | Self::Split { .. } | Self::Unified { .. } => {
                px(DIFF_ROW_HEIGHT)
            }
            Self::Empty { .. } => px(180.),
        }
    }

    const fn file_index(&self) -> Option<usize> {
        match self {
            Self::FileGap => None,
            Self::FileHeader { file_index }
            | Self::Separator { file_index, .. }
            | Self::Split { file_index, .. }
            | Self::Unified { file_index, .. }
            | Self::Empty { file_index } => Some(*file_index as usize),
        }
    }
}

#[derive(Clone)]
struct FileViewMeta {
    display_path: SharedString,
    absolute_path: Option<SharedString>,
    relative_path: Option<SharedString>,
    header_path: SharedString,
    header_file_name_start: usize,
    file_name: SharedString,
    language: &'static str,
    change_kind: FileChangeKind,
}

impl FileViewMeta {
    fn new(file: &DiffFile, path_root: Option<&Path>) -> Self {
        let mut header_path = if file.parent_path().is_empty() {
            String::new()
        } else {
            format!("{} / ", file.parent_path())
        };
        let header_file_name_start = header_path.len();
        header_path.push_str(file.file_name());
        let display_path = Path::new(file.display_path());
        let relative_path =
            (!display_path.is_absolute()).then(|| file.display_path().to_owned().into());
        let absolute_path = if display_path.is_absolute() {
            Some(display_path.to_owned())
        } else {
            path_root.map(|root| root.join(display_path))
        }
        .map(|path| path.to_string_lossy().into_owned().into());
        Self {
            display_path: file.display_path().to_owned().into(),
            absolute_path,
            relative_path,
            header_path: header_path.into(),
            header_file_name_start,
            file_name: file.file_name().to_owned().into(),
            language: detected_language_for_file(file),
            change_kind: FileChangeKind::for_file(file),
        }
    }
}

struct FileViewData {
    meta: Vec<FileViewMeta>,
    stats: Vec<(usize, usize)>,
    total_additions: usize,
    total_deletions: usize,
}

impl FileViewData {
    fn from_files(files: &[DiffFile], path_root: Option<&Path>) -> Self {
        let mut meta = Vec::with_capacity(files.len());
        let mut stats = Vec::with_capacity(files.len());
        let mut total_additions = 0;
        let mut total_deletions = 0;
        for file in files {
            let additions = file.additions();
            let deletions = file.deletions();
            meta.push(FileViewMeta::new(file, path_root));
            stats.push((additions, deletions));
            total_additions += additions;
            total_deletions += deletions;
        }
        Self {
            meta,
            stats,
            total_additions,
            total_deletions,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum FileTreeRow {
    Directory {
        path: SharedString,
        name: SharedString,
        depth: usize,
        expanded: bool,
    },
    File {
        file_index: usize,
        depth: usize,
    },
}

impl FileTreeRow {
    const fn height(&self) -> Pixels {
        match self {
            Self::Directory { .. } => px(TREE_DIRECTORY_ROW_HEIGHT),
            Self::File { .. } => px(TREE_FILE_ROW_HEIGHT),
        }
    }
}

#[derive(Default)]
struct FileTreeNode {
    directories: BTreeMap<String, Self>,
    files: Vec<(String, usize)>,
    first_file_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
}

impl FileChangeKind {
    fn for_file(file: &DiffFile) -> Self {
        if file.is_new {
            Self::Added
        } else if file.is_deleted {
            Self::Deleted
        } else if trim_diff_prefix(&file.old_path) != trim_diff_prefix(&file.new_path) {
            Self::Renamed
        } else {
            Self::Modified
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
        }
    }

    const fn color(self, palette: Palette) -> Rgba {
        match self {
            Self::Modified => palette.yellow,
            Self::Added => palette.green,
            Self::Deleted => palette.red,
            Self::Renamed => palette.blue,
        }
    }
}

struct DiffViewer {
    diff: DiffSet,
    path_root: Option<PathBuf>,
    source_name: String,
    comparison_label: String,
    target_label: String,
    empty_title: String,
    empty_detail: String,
    selected_file: usize,
    layout: DiffLayout,
    theme: ThemeMode,
    text_selection: Option<TextSelection>,
    header_text_selection: Option<HeaderTextSelection>,
    text_context_menu: Option<Point<Pixels>>,
    path_context_menu: Option<PathContextMenu>,
    source_switcher: Option<GitDiffSourceSwitcher>,
    source_picker_open: bool,
    source_picker_section: SourcePickerSection,
    source_picker_scroll: ScrollHandle,
    source_catalog: Option<GitSourceCatalog>,
    source_catalog_load_state: SourceCatalogLoadState,
    source_catalog_generation: u64,
    source_error: Option<String>,
    copy_path_feedback: Option<CopyPathFeedback>,
    copy_path_feedback_generation: u64,
    focus_handle: FocusHandle,
    file_scroll: ScrollHandle,
    diff_scroll: ScrollHandle,
    diff_font_size: Pixels,
    diff_font_size_target: Pixels,
    diff_font_animation_target: Pixels,
    diff_layout_font_size: Pixels,
    diff_font_zoom_running: bool,
    diff_font_zoom_last_frame: Option<Instant>,
    diff_row_offsets_target: Option<Vec<Pixels>>,
    diff_row_offsets_progress: f32,
    diff_layout_zoom_start_font_size: Pixels,
    diff_layout_zoom_anchor: Option<DiffZoomAnchor>,
    file_smooth_scroll: SmoothScrollState,
    diff_smooth_scroll: SmoothScrollState,
    scrollbar_drag: Option<ScrollbarDrag>,
    file_sidebar_hovered: bool,
    sidebar_width: Pixels,
    sidebar_resize_drag: Option<SidebarResizeDrag>,
    middle_auto_scroll: Option<MiddleAutoScroll>,
    diff_rows: DiffRowIndex,
    diff_row_layouts: Vec<DiffRowLayout>,
    diff_row_offsets: Vec<Pixels>,
    file_row_indices: Vec<usize>,
    diff_row_context_gaps: Vec<ContextGap>,
    file_tree_rows: Vec<FileTreeRow>,
    file_tree_row_offsets: Vec<Pixels>,
    collapsed_directories: HashSet<String>,
    collapsed_files: HashSet<usize>,
    viewed_files: HashSet<String>,
    context_expansions: HashMap<ContextGap, ContextExpansion>,
    pending_scroll_file: Option<usize>,
    pending_diff_zoom_anchor: Option<DiffZoomAnchor>,
    wrapped_offsets_range: Option<WrapWidthRange>,
    syntax_cache: HashMap<SyntaxCacheKey, CachedCodeLine>,
    syntax_streams: HashMap<SyntaxStreamKey, SyntaxStreamState>,
    inline_cache: HashMap<InlineCacheKey, CachedInlineHighlightPair>,
    file_meta: Vec<FileViewMeta>,
    file_stats: Vec<(usize, usize)>,
    total_additions: usize,
    total_deletions: usize,
    _window_activation_subscription: Subscription,
}

impl DiffViewer {
    fn new(
        input: Input,
        watch: Option<WatchRequest>,
        source_switcher: Option<GitDiffSourceSwitcher>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let Input {
            diff,
            path_root,
            source_name,
            comparison_label,
            target_label,
            empty_title,
            empty_detail,
        } = input;
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        let FileViewData {
            meta: file_meta,
            stats: file_stats,
            total_additions,
            total_deletions,
        } = FileViewData::from_files(&diff.files, path_root.as_deref());
        let collapsed_files = HashSet::new();
        let context_expansions = HashMap::new();
        let DiffRowData {
            rows: diff_rows,
            layouts: diff_row_layouts,
            file_row_indices,
            offsets: diff_row_offsets,
            context_gaps: diff_row_context_gaps,
        } = build_diff_row_data(
            &diff,
            DEFAULT_DIFF_LAYOUT,
            &collapsed_files,
            &context_expansions,
        );
        let FileTreeData {
            rows: file_tree_rows,
            offsets: file_tree_row_offsets,
        } = build_file_tree_data(&diff.files, &HashSet::new());
        let window_activation_subscription =
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.cancel_middle_auto_scroll(cx);
                    this.finish_sidebar_resize(cx);
                }
            });
        let viewer = Self {
            diff,
            path_root,
            source_name,
            comparison_label,
            target_label,
            empty_title,
            empty_detail,
            selected_file: 0,
            layout: DEFAULT_DIFF_LAYOUT,
            theme: ThemeMode::Dark,
            text_selection: None,
            header_text_selection: None,
            text_context_menu: None,
            path_context_menu: None,
            source_switcher,
            source_picker_open: false,
            source_picker_section: SourcePickerSection::default(),
            source_picker_scroll: ScrollHandle::new(),
            source_catalog: None,
            source_catalog_load_state: SourceCatalogLoadState::default(),
            source_catalog_generation: 0,
            source_error: None,
            copy_path_feedback: None,
            copy_path_feedback_generation: 0,
            focus_handle,
            file_scroll: ScrollHandle::new(),
            diff_scroll: ScrollHandle::new(),
            diff_font_size: px(DEFAULT_DIFF_FONT_SIZE),
            diff_font_size_target: px(DEFAULT_DIFF_FONT_SIZE),
            diff_font_animation_target: px(DEFAULT_DIFF_FONT_SIZE),
            diff_layout_font_size: px(DEFAULT_DIFF_FONT_SIZE),
            diff_font_zoom_running: false,
            diff_font_zoom_last_frame: None,
            diff_row_offsets_target: None,
            diff_row_offsets_progress: 0.,
            diff_layout_zoom_start_font_size: px(DEFAULT_DIFF_FONT_SIZE),
            diff_layout_zoom_anchor: None,
            file_smooth_scroll: SmoothScrollState::default(),
            diff_smooth_scroll: SmoothScrollState::default(),
            scrollbar_drag: None,
            file_sidebar_hovered: false,
            sidebar_width: px(DEFAULT_SIDEBAR_WIDTH),
            sidebar_resize_drag: None,
            middle_auto_scroll: None,
            diff_rows,
            diff_row_layouts,
            diff_row_offsets,
            file_row_indices,
            diff_row_context_gaps,
            file_tree_rows,
            file_tree_row_offsets,
            collapsed_directories: HashSet::new(),
            collapsed_files,
            viewed_files: HashSet::new(),
            context_expansions,
            pending_scroll_file: None,
            pending_diff_zoom_anchor: None,
            wrapped_offsets_range: None,
            syntax_cache: HashMap::new(),
            syntax_streams: HashMap::new(),
            inline_cache: HashMap::new(),
            file_meta,
            file_stats,
            total_additions,
            total_deletions,
            _window_activation_subscription: window_activation_subscription,
        };
        if let Some(watch) = watch
            && let Err(error) = viewer.start_watch(watch, cx)
        {
            eprintln!("Could not watch diff: {error}");
        }
        viewer
    }

    fn palette(&self) -> Palette {
        Palette::for_mode(self.theme)
    }

    fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.theme = match self.theme {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        };
        self.syntax_cache.clear();
        self.syntax_streams.clear();
        cx.notify();
    }

    fn update_diff_row_offsets(&mut self, window: &Window, palette: Palette) {
        if self.sidebar_resize_drag.is_some() {
            return;
        }
        let code_width = wrapped_code_width(
            window.viewport_size().width,
            self.sidebar_width,
            self.layout,
            self.diff_layout_font_size,
        )
        .floor();
        if self
            .wrapped_offsets_range
            .is_some_and(|range| range.contains(code_width))
        {
            return;
        }
        let resize_anchor = self
            .wrapped_offsets_range
            .and_then(|_| self.current_diff_zoom_anchor());
        self.cancel_diff_layout_zoom();
        let (offsets, width_range) = self.calculate_wrapped_row_offsets(
            code_width,
            self.diff_layout_font_size,
            window,
            palette,
        );
        self.diff_row_offsets = offsets;
        self.wrapped_offsets_range = Some(width_range);
        if let Some(anchor) = self.pending_diff_zoom_anchor.take() {
            self.restore_diff_zoom_anchor(anchor);
        } else if let Some(file_index) = self.pending_scroll_file.take() {
            self.scroll_to_file(file_index);
        } else if let Some(anchor) = resize_anchor {
            self.restore_diff_zoom_anchor(anchor);
        }
    }

    fn diff_offsets(&self) -> InterpolatedOffsets<'_> {
        InterpolatedOffsets::new(
            &self.diff_row_offsets,
            self.diff_row_offsets_target.as_deref(),
            self.diff_row_offsets_progress,
        )
    }
}

impl Render for DiffViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette();
        self.update_diff_row_offsets(window, palette);
        self.sync_selected_file_from_scroll();
        let middle_scroll_cursor = if cfg!(target_os = "windows") {
            None
        } else {
            self.middle_auto_scroll
                .map(|state| vertical_auto_scroll_cursor(state.cursor.y - state.anchor.y))
        };

        let content = if self.diff.files.is_empty() {
            self.render_empty(palette)
        } else {
            self.render_diff_body(palette, cx)
        };
        let text_context_menu = self.text_context_menu.map(|position| {
            self.render_text_context_menu(position, window.viewport_size(), palette, cx)
        });
        let path_context_menu = self
            .path_context_menu
            .as_ref()
            .map(|menu| self.render_path_context_menu(menu, window.viewport_size(), palette, cx));
        let source_picker = self
            .source_picker_open
            .then(|| self.render_source_picker(palette, cx));

        div()
            .key_context("DiffViewer")
            .track_focus(&self.focus_handle)
            .on_any_mouse_down(cx.listener(|this, event, _, cx| {
                this.cancel_middle_auto_scroll_on_mouse_down(event, cx);
            }))
            .on_mouse_move(cx.listener(|this, event, window, cx| {
                this.update_middle_auto_scroll_cursor(event, cx);
                this.update_sidebar_resize(event, window, cx);
                this.update_text_selection(event, window, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.finish_sidebar_resize(cx);
                    this.finish_text_selection(cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.finish_sidebar_resize(cx);
                    this.finish_text_selection(cx);
                }),
            )
            .on_action(cx.listener(|this, _: &ToggleLayout, _, cx| this.toggle_layout(cx)))
            .on_action(cx.listener(|this, _: &ToggleTheme, _, cx| this.toggle_theme(cx)))
            .on_action(cx.listener(|this, _: &ToggleViewed, _, cx| {
                this.toggle_selected_file_viewed(cx);
            }))
            .on_action(cx.listener(|this, _: &NextFile, _, cx| this.next_file(cx)))
            .on_action(cx.listener(|this, _: &PreviousFile, _, cx| this.previous_file(cx)))
            .on_action(
                cx.listener(|this, _: &CopyTextSelection, _, cx| this.copy_text_selection(cx)),
            )
            .size_full()
            .relative()
            .overflow_hidden()
            .font_family("Segoe UI")
            .text_size(px(13.))
            .text_color(palette.text)
            .bg(palette.canvas)
            .when_some(middle_scroll_cursor, gpui::Styled::cursor)
            .when(self.sidebar_resize_drag.is_some(), |this| {
                this.cursor(CursorStyle::ResizeLeftRight)
            })
            .flex()
            .flex_col()
            .child(self.render_top_bar(palette, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_sidebar(palette, cx))
                    .child(self.render_sidebar_resize_handle(palette, cx))
                    .child(content),
            )
            .children(text_context_menu)
            .children(path_context_menu)
            .children(source_picker)
    }
}

pub(super) fn run(launch: Launch) {
    let Launch {
        input,
        watch,
        source_switcher,
    } = launch;
    Application::new().run(move |cx: &mut App| {
        crate::application_icon::install();
        cx.bind_keys([
            KeyBinding::new("s", ToggleLayout, Some("DiffViewer")),
            KeyBinding::new("shift-t", ToggleTheme, Some("DiffViewer")),
            KeyBinding::new("v", ToggleViewed, Some("DiffViewer")),
            KeyBinding::new("down", NextFile, Some("DiffViewer")),
            KeyBinding::new("up", PreviousFile, Some("DiffViewer")),
            KeyBinding::new("ctrl-c", CopyTextSelection, Some("DiffViewer")),
        ]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Sabun".into()),
                    ..Default::default()
                }),
                focus: true,
                ..Default::default()
            },
            move |window, cx| {
                cx.new(|cx| {
                    DiffViewer::new(
                        input.clone(),
                        watch.clone(),
                        source_switcher.clone(),
                        window,
                        cx,
                    )
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
