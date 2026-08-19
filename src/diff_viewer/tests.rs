use gpui::{CursorStyle, HighlightStyle, ScrollDelta, point, px, rgb, size};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use super::text_selection::TextPoint;
use crate::diff::{DiffFile, DiffHunk, DiffSet, LineKind};

use super::{
    ContextExpandDirection, ContextExpansion, ContextGap, ContextGapPosition, ContextGapSource,
    DIFF_ROW_HEIGHT, DiffDisplayRow, DiffLayout, FileChangeKind, FileTreeRow, HeaderTextSelection,
    InterpolatedOffsets, Palette, TextLane, TextSelection, ThemeMode, WHEEL_PIXELS_PER_LINE,
    accumulate_scroll_target, apply_selection_background, build_diff_row_data, build_diff_rows,
    build_file_diff_rows, build_file_tree_rows, clamp_context_menu_position, clamped_sidebar_width,
    collapse_file_rows, detected_language, detected_language_for_file, diff_row_layouts,
    file_tree_row_offsets, inline_ranges, middle_auto_scroll_velocity, paired_line, row_offsets,
    selection_padding_edges, sticky_file_tree_directories, syntax_highlighter, syntax_highlights,
    syntax_highlights_with_state, syntax_set, syntax_theme, unpack_diff_row_index,
    vertical_auto_scroll_cursor, wheel_zoom_direction, windows_vertical_pan_cursor_id,
};

#[test]
fn original_syntax_themes_are_defaults_and_supply_diff_colors() {
    let themes = syntect::highlighting::ThemeSet::load_defaults();
    assert_eq!(
        syntax_theme(ThemeMode::Dark),
        &themes.themes["base16-eighties.dark"]
    );
    assert_eq!(
        syntax_theme(ThemeMode::Light),
        &themes.themes["InspiredGitHub"]
    );
    let light_palette = Palette::for_mode(ThemeMode::Light);
    assert_eq!(light_palette.canvas, rgb(0xffffff));
    assert_eq!(light_palette.green, rgb(0x16855b));
    assert_eq!(light_palette.green_bg, rgb(0xeaffea));
    assert_eq!(light_palette.red, rgb(0xc83f54));
    assert_eq!(light_palette.red_bg, rgb(0xffecec));
}

fn file_diff_rows_for_test(
    file: &DiffFile,
    layout: DiffLayout,
    expansions: &HashMap<ContextGap, ContextExpansion>,
) -> (Vec<DiffDisplayRow>, Vec<ContextGap>) {
    let mut context_gaps = Vec::new();
    let rows = build_file_diff_rows(file, 0, layout, expansions, &mut context_gaps);
    (rows.iter().collect(), context_gaps)
}

#[test]
fn zoomed_row_offsets_are_interpolated_without_materializing_every_row() {
    let base = [px(0.), px(10.), px(20.), px(30.)];
    let target = [px(0.), px(20.), px(40.), px(60.)];
    let offsets = InterpolatedOffsets::new(&base, Some(&target), 0.25);

    assert_eq!(offsets.get(2), Some(px(25.)));
    assert_eq!(offsets.row_height(1), px(12.5));
    assert_eq!(offsets.row_index_at(px(24.), 3), 1);
    assert_eq!(offsets.row_index_at(px(25.), 3), 2);
    assert_eq!(offsets.last(), Some(px(37.5)));
}

#[test]
fn sidebar_resize_preserves_both_panes_minimum_width() {
    assert_eq!(clamped_sidebar_width(px(100.), px(1_000.)), px(180.));
    assert_eq!(clamped_sidebar_width(px(300.), px(1_000.)), px(300.));
    assert_eq!(clamped_sidebar_width(px(700.), px(800.)), px(435.));
}

#[test]
fn text_context_menu_stays_inside_the_window() {
    let viewport = size(px(800.), px(600.));
    assert_eq!(
        clamp_context_menu_position(point(px(790.), px(590.)), viewport),
        point(px(616.), px(556.))
    );
    assert_eq!(
        clamp_context_menu_position(point(px(-10.), px(-20.)), viewport),
        point(px(4.), px(4.))
    );
}

#[test]
fn text_selection_ranges_span_lines_in_both_drag_directions() {
    let forward = TextSelection {
        anchor: TextPoint {
            display_index: 2,
            lane: TextLane::Unified,
            offset: 3,
        },
        head: TextPoint {
            display_index: 4,
            lane: TextLane::Unified,
            offset: 2,
        },
        dragging: false,
    };
    assert_eq!(forward.range_for(2, TextLane::Unified, 8), Some(3..8));
    assert_eq!(forward.range_for(3, TextLane::Unified, 6), Some(0..6));
    assert_eq!(forward.range_for(4, TextLane::Unified, 7), Some(0..2));
    assert_eq!(forward.range_for(3, TextLane::Old, 6), None);
    assert_eq!(forward.range_for(3, TextLane::Unified, 0), Some(0..0));

    let reverse = TextSelection {
        anchor: forward.head,
        head: forward.anchor,
        dragging: false,
    };
    assert_eq!(reverse.range_for(2, TextLane::Unified, 8), Some(3..8));
    assert_eq!(reverse.range_for(4, TextLane::Unified, 7), Some(0..2));
}

#[test]
fn header_text_selection_range_handles_both_drag_directions() {
    let forward = HeaderTextSelection {
        file_index: 2,
        sticky_header: false,
        anchor: 3,
        head: 9,
        dragging: false,
    };
    assert_eq!(forward.range(12), Some(3..9));
    assert_eq!(
        HeaderTextSelection {
            anchor: 9,
            head: 3,
            ..forward
        }
        .range(12),
        Some(3..9)
    );
    assert_eq!(
        HeaderTextSelection {
            anchor: 3,
            head: 3,
            ..forward
        }
        .range(12),
        None
    );
    assert_eq!(forward.range(5), Some(3..5));
    assert_eq!(forward.range(2), None);
}

#[test]
fn text_selection_background_overrides_inline_diff_background() {
    let selection_color = rgb(0x303866);
    let syntax_color = rgb(0xff5577);
    let inline_color = rgb(0x184e38);
    let highlights = vec![
        (
            0..8,
            HighlightStyle {
                color: Some(syntax_color.into()),
                ..Default::default()
            },
        ),
        (
            2..5,
            HighlightStyle {
                background_color: Some(inline_color.into()),
                ..Default::default()
            },
        ),
    ];

    let highlights = apply_selection_background(highlights, 1..7, selection_color);

    for offset in 1..7 {
        assert!(highlights.iter().any(|(range, style)| {
            range.contains(&offset) && style.background_color == Some(selection_color.into())
        }));
    }
    assert!(highlights.iter().any(|(range, style)| {
        range.start < 7 && 1 < range.end && style.color == Some(syntax_color.into())
    }));
}

#[test]
fn multiline_text_selection_bridges_only_across_selected_line_edges() {
    assert_eq!(selection_padding_edges(&(0..8), 8), (true, true));
    assert_eq!(selection_padding_edges(&(3..8), 8), (false, true));
    assert_eq!(selection_padding_edges(&(0..5), 8), (true, false));
    assert_eq!(selection_padding_edges(&(2..5), 8), (false, false));
}

#[test]
fn bundled_syntax_pack_detects_modern_project_files() {
    assert!(syntax_set().syntaxes().len() >= 100);
    assert_eq!(detected_language("src/App.vue"), "Vue Component");

    for path in [
        "src/App.svelte",
        "src/Widget.tsx",
        "Dockerfile",
        "Makefile",
        ".env",
        "flake.nix",
        "infra/main.tf",
        "shader.wgsl",
        ".github/workflows/ci.yml",
    ] {
        assert_ne!(detected_language(path), "Plain Text", "{path}");
    }
}

#[test]
fn vue_markup_receives_syntax_highlights() {
    let markup_highlights = syntax_highlights(
        r#"<template><button @click="count++">{{ count }}</button></template>"#,
        detected_language("Counter.vue"),
        Palette::for_mode(ThemeMode::Dark),
    );
    assert!(markup_highlights.len() >= 3);

    let script_highlights = syntax_highlights(
        "const count = ref(0);",
        detected_language("Counter.vue"),
        Palette::for_mode(ThemeMode::Dark),
    );
    assert!(script_highlights.len() >= 3);
}

#[test]
fn stateful_syntax_highlighting_carries_multiline_context() {
    let palette = Palette::for_mode(ThemeMode::Dark);
    let mut highlighter = syntax_highlighter("Rust", ThemeMode::Dark);
    let mut embedded_highlighter = None;
    let mut line_buffer = String::new();
    let _ = syntax_highlights_with_state(
        &mut highlighter,
        &mut embedded_highlighter,
        &mut line_buffer,
        "/* comment starts",
        "Rust",
        palette,
    );
    let continued = syntax_highlights_with_state(
        &mut highlighter,
        &mut embedded_highlighter,
        &mut line_buffer,
        "still a comment */ let value = 1;",
        "Rust",
        palette,
    );
    let standalone = syntax_highlights("still a comment */ let value = 1;", "Rust", palette);

    assert_ne!(continued, standalone);
}

#[test]
fn extensionless_scripts_are_detected_from_the_shebang() {
    let file = DiffFile {
        old_path: "a/tools/release".into(),
        new_path: "b/tools/release".into(),
        hunks: vec![DiffHunk::from_lines(
            "@@ -1 +1 @@",
            1,
            1,
            [(
                Some(1),
                Some(1),
                LineKind::Context,
                "#!/usr/bin/env python3",
            )],
        )],
        is_new: false,
        is_deleted: false,
    };

    assert_eq!(detected_language_for_file(&file), "Python");
}

#[test]
fn smooth_scroll_reverses_from_the_visible_position() {
    let current = point(px(0.), px(-120.));
    let queued = point(px(0.), px(-240.));
    let max_offset = size(px(0.), px(1_000.));

    let reversed = accumulate_scroll_target(current, queued, point(px(0.), px(60.)), max_offset);
    assert_eq!(reversed.y, px(-60.));

    let continued = accumulate_scroll_target(current, queued, point(px(0.), px(-60.)), max_offset);
    assert_eq!(continued.y, px(-300.));
}

#[test]
fn wheel_lines_use_a_browser_sized_step() {
    let delta = ScrollDelta::Lines(point(0., 3.)).pixel_delta(px(WHEEL_PIXELS_PER_LINE));
    assert_eq!(delta.y, px(96.));
}

#[test]
fn control_wheel_zoom_follows_vertical_wheel_direction() {
    assert_eq!(wheel_zoom_direction(ScrollDelta::Lines(point(0., 3.))), 1);
    assert_eq!(wheel_zoom_direction(ScrollDelta::Lines(point(0., -3.))), -1);
    assert_eq!(
        wheel_zoom_direction(ScrollDelta::Pixels(point(px(0.), px(0.)))),
        0
    );
}

#[test]
fn middle_auto_scroll_matches_chromiums_acceleration_curve() {
    assert_eq!(middle_auto_scroll_velocity(px(15.)), px(0.));
    assert_eq!(middle_auto_scroll_velocity(px(-15.)), px(0.));
    assert!(middle_auto_scroll_velocity(px(80.)) > px(0.));
    assert!(middle_auto_scroll_velocity(px(-80.)) < px(0.));
    assert!(middle_auto_scroll_velocity(px(400.)) > px(4_000.));
    assert!(middle_auto_scroll_velocity(px(1_000.)) > px(30_000.));
}

#[test]
fn middle_auto_scroll_cursor_only_points_vertically() {
    assert_eq!(vertical_auto_scroll_cursor(px(-80.)), CursorStyle::ResizeUp);
    assert_eq!(
        vertical_auto_scroll_cursor(px(0.)),
        CursorStyle::ResizeUpDown
    );
    assert_eq!(
        vertical_auto_scroll_cursor(px(80.)),
        CursorStyle::ResizeDown
    );
    assert_eq!(windows_vertical_pan_cursor_id(px(-80.)), 32655);
    assert_eq!(windows_vertical_pan_cursor_id(px(0.)), 32652);
    assert_eq!(windows_vertical_pan_cursor_id(px(80.)), 32656);
}

#[test]
fn flattened_diff_rows_keep_exact_virtual_height() {
    let file = DiffFile {
        old_path: "a/src/main.rs".into(),
        new_path: "b/src/main.rs".into(),
        hunks: vec![DiffHunk::from_lines(
            "@@ -1,3 +1,3 @@",
            1,
            1,
            (1..=3).map(|number| {
                (
                    Some(number),
                    Some(number),
                    LineKind::Context,
                    format!("line {number}"),
                )
            }),
        )],
        is_new: false,
        is_deleted: false,
    };

    let (rows, _) = file_diff_rows_for_test(&file, DiffLayout::Split, &HashMap::new());
    let offsets = super::row_geometry::cumulative_offsets(&rows, DiffDisplayRow::height);
    assert_eq!(rows.len(), 3);
    assert_eq!(offsets.len(), 4);
    assert_eq!(offsets[3], px(DIFF_ROW_HEIGHT * 3.));
}

#[test]
fn all_files_share_one_virtualized_row_stream() {
    let file = DiffFile {
        old_path: "a/src/main.rs".into(),
        new_path: "b/src/main.rs".into(),
        hunks: vec![DiffHunk::from_lines(
            "@@ -1 +1 @@",
            1,
            1,
            [(Some(1), Some(1), LineKind::Context, "fn main() {}")],
        )],
        is_new: false,
        is_deleted: false,
    };

    let (rows, file_starts, _) = build_diff_rows(
        &[file.clone(), file],
        DiffLayout::Unified,
        &HashSet::new(),
        &HashMap::new(),
    );
    assert_eq!(file_starts, vec![0, 3]);
    assert!(matches!(
        rows.get(file_starts[0]).unwrap(),
        DiffDisplayRow::FileHeader { file_index: 0 }
    ));
    assert!(matches!(rows.get(2), Some(DiffDisplayRow::FileGap)));
    assert!(matches!(
        rows.get(file_starts[1]).unwrap(),
        DiffDisplayRow::FileHeader { file_index: 1 }
    ));
    assert!(
        rows.iter()
            .skip(file_starts[1])
            .filter_map(|row| row.file_index())
            .all(|index| index == 1)
    );
}

#[test]
fn collapsed_diff_file_keeps_its_header_and_hides_its_body() {
    let file = DiffFile {
        old_path: "a/src/main.rs".into(),
        new_path: "b/src/main.rs".into(),
        hunks: vec![DiffHunk::from_lines(
            "@@ -1 +1 @@",
            1,
            1,
            [(Some(1), Some(1), LineKind::Context, "fn main() {}")],
        )],
        is_new: false,
        is_deleted: false,
    };

    let (rows, file_starts, _) = build_diff_rows(
        &[file.clone(), file],
        DiffLayout::Unified,
        &HashSet::from([0]),
        &HashMap::new(),
    );
    assert_eq!(file_starts, vec![0, 1]);
    assert!(matches!(
        rows.get(0).unwrap(),
        DiffDisplayRow::FileHeader { file_index: 0 }
    ));
    assert!(
        rows.iter()
            .take(file_starts[1])
            .filter_map(|row| row.file_index())
            .all(|index| index == 0)
    );
    assert!(matches!(
        rows.get(file_starts[1]).unwrap(),
        DiffDisplayRow::FileHeader { file_index: 1 }
    ));
}

#[test]
fn collapsing_diff_file_removes_only_its_body_without_rebuilding_other_rows() {
    let file = DiffFile {
        old_path: "a/src/main.rs".into(),
        new_path: "b/src/main.rs".into(),
        hunks: vec![DiffHunk::from_lines(
            "@@ -1 +1 @@",
            1,
            1,
            [(Some(1), Some(1), LineKind::Context, "fn main() {}")],
        )],
        is_new: false,
        is_deleted: false,
    };
    let files = [file.clone(), file];
    let (mut rows, mut file_starts, _) = build_diff_rows(
        &files,
        DiffLayout::Unified,
        &HashSet::new(),
        &HashMap::new(),
    );
    let mut offsets = row_offsets(&rows);
    let mut layouts = diff_row_layouts(&rows, &files);
    let second_header = rows.get(file_starts[1]).unwrap();

    let removed =
        collapse_file_rows(&mut rows, &mut layouts, &mut offsets, &mut file_starts, 0).unwrap();

    assert_eq!(removed, 1..3);
    assert_eq!(file_starts, vec![0, 1]);
    assert_eq!(rows.len() + 1, offsets.len());
    assert!(matches!(
        rows.get(0).unwrap(),
        DiffDisplayRow::FileHeader { file_index: 0 }
    ));
    assert!(matches!(
        (rows.get(1).unwrap(), second_header),
        (
            DiffDisplayRow::FileHeader { file_index: 1 },
            DiffDisplayRow::FileHeader { file_index: 1 }
        )
    ));
}

#[test]
fn changed_files_form_a_collapsible_path_tree() {
    let file = |path: &str| DiffFile {
        old_path: format!("a/{path}"),
        new_path: format!("b/{path}"),
        hunks: Vec::new(),
        is_new: false,
        is_deleted: false,
    };
    let files = vec![
        file("src/components/button.rs"),
        file("src/lib.rs"),
        file("README.md"),
    ];

    let expanded = build_file_tree_rows(&files, &HashSet::new());
    assert!(matches!(
        &expanded[0],
        FileTreeRow::Directory { path, depth: 0, .. } if path == "src"
    ));
    assert!(matches!(
        &expanded[1],
        FileTreeRow::Directory { path, depth: 1, .. } if path == "src/components"
    ));
    assert!(matches!(
        expanded[2],
        FileTreeRow::File {
            file_index: 0,
            depth: 2
        }
    ));

    let collapsed = build_file_tree_rows(&files, &HashSet::from(["src".to_owned()]));
    assert_eq!(collapsed.len(), 2);
    assert!(matches!(
        &collapsed[0],
        FileTreeRow::Directory {
            path,
            expanded: false,
            ..
        } if path == "src"
    ));
    assert!(matches!(
        collapsed[1],
        FileTreeRow::File { file_index: 2, .. }
    ));
}

#[test]
fn unchanged_context_is_collapsed_and_can_be_expanded() {
    let line = |kind, content: &str| (None, None, kind, content.to_owned());
    let mut lines = (0..3)
        .map(|index| line(LineKind::Context, &format!("leading {index}")))
        .collect::<Vec<_>>();
    lines.push(line(LineKind::Deletion, "old first"));
    lines.push(line(LineKind::Addition, "new first"));
    lines.extend((0..70).map(|index| line(LineKind::Context, &format!("unchanged {index}"))));
    lines.push(line(LineKind::Deletion, "old second"));
    lines.push(line(LineKind::Addition, "new second"));
    lines.extend((0..3).map(|index| line(LineKind::Context, &format!("trailing {index}"))));
    let file = DiffFile {
        old_path: "a/file.rs".into(),
        new_path: "b/file.rs".into(),
        hunks: vec![DiffHunk::from_lines("@@ -1,78 +1,78 @@", 1, 1, lines)],
        is_new: false,
        is_deleted: false,
    };

    let (collapsed, context_gaps) =
        file_diff_rows_for_test(&file, DiffLayout::Unified, &HashMap::new());
    let gap = collapsed
        .iter()
        .find_map(|row| match row {
            DiffDisplayRow::Separator {
                hidden: 64,
                gap_index,
                ..
            } => unpack_diff_row_index(*gap_index)
                .and_then(|index| context_gaps.get(index))
                .copied(),
            _ => None,
        })
        .unwrap();
    assert_eq!(collapsed.len(), 17);
    assert_eq!(
        collapsed
            .iter()
            .find(|row| matches!(row, DiffDisplayRow::Separator { .. }))
            .unwrap()
            .height(),
        px(DIFF_ROW_HEIGHT * 2.)
    );

    let mut expansion = ContextExpansion::default();
    assert!(expansion.reveal(gap, ContextExpandDirection::Down));
    assert_eq!(expansion.from_start, 30);
    let (partially_expanded, _) = file_diff_rows_for_test(
        &file,
        DiffLayout::Unified,
        &HashMap::from([(gap, expansion)]),
    );
    assert_eq!(partially_expanded.len(), 47);
    assert!(
        partially_expanded
            .iter()
            .any(|row| matches!(row, DiffDisplayRow::Separator { hidden: 34, .. }))
    );

    let (expanded, _) = file_diff_rows_for_test(
        &file,
        DiffLayout::Unified,
        &HashMap::from([(
            gap,
            ContextExpansion {
                from_start: gap.end - gap.start,
                from_end: 0,
            },
        )]),
    );
    assert_eq!(expanded.len(), 80);
    assert!(
        !expanded
            .iter()
            .any(|row| matches!(row, DiffDisplayRow::Separator { .. }))
    );
}

#[test]
fn unchanged_context_at_file_edges_can_be_expanded() {
    let line = |kind, content: &str| (None, None, kind, content.to_owned());
    let mut lines = (0..10)
        .map(|index| line(LineKind::Context, &format!("leading {index}")))
        .collect::<Vec<_>>();
    lines.push(line(LineKind::Deletion, "old"));
    lines.push(line(LineKind::Addition, "new"));
    lines.extend((0..10).map(|index| line(LineKind::Context, &format!("trailing {index}"))));
    let file = DiffFile {
        old_path: "a/file.rs".into(),
        new_path: "b/file.rs".into(),
        hunks: vec![DiffHunk::from_lines("@@ -1,21 +1,21 @@", 1, 1, lines)],
        is_new: false,
        is_deleted: false,
    };

    let (collapsed, context_gaps) =
        file_diff_rows_for_test(&file, DiffLayout::Unified, &HashMap::new());
    let gaps = collapsed
        .iter()
        .filter_map(|row| match row {
            DiffDisplayRow::Separator {
                hidden: 7,
                gap_index,
                ..
            } => unpack_diff_row_index(*gap_index)
                .and_then(|index| context_gaps.get(index))
                .copied(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(gaps.len(), 2);
    assert_eq!(collapsed.len(), 10);
    assert!(matches!(collapsed[0], DiffDisplayRow::Separator { .. }));
    assert_eq!(collapsed[0].height(), px(DIFF_ROW_HEIGHT));
    assert!(matches!(
        collapsed.last(),
        Some(DiffDisplayRow::Separator { .. })
    ));

    let expansions = gaps
        .into_iter()
        .map(|gap| {
            (
                gap,
                ContextExpansion {
                    from_start: gap.end - gap.start,
                    from_end: 0,
                },
            )
        })
        .collect();
    let (expanded, _) = file_diff_rows_for_test(&file, DiffLayout::Unified, &expansions);
    assert_eq!(expanded.len(), 22);
    assert!(
        !expanded
            .iter()
            .any(|row| matches!(row, DiffDisplayRow::Separator { .. }))
    );
}

#[test]
fn git_context_gaps_expand_from_source_without_eager_rows() {
    let source = (1..=100).fold(String::new(), |mut source, number| {
        writeln!(source, "line {number}").unwrap();
        source
    });
    let file = DiffFile {
        old_path: "a/file.rs".into(),
        new_path: "b/file.rs".into(),
        hunks: vec![
            DiffHunk::from_lines(
                "@@ -4,3 +4,3 @@",
                4,
                4,
                [
                    (Some(4), Some(4), LineKind::Context, "line 4"),
                    (Some(5), None, LineKind::Deletion, "line 5"),
                    (None, Some(5), LineKind::Addition, "changed 5"),
                    (Some(6), Some(6), LineKind::Context, "line 6"),
                ],
            ),
            DiffHunk::from_lines(
                "@@ -95,3 +95,3 @@",
                95,
                95,
                [
                    (Some(95), Some(95), LineKind::Context, "line 95"),
                    (Some(96), None, LineKind::Deletion, "line 96"),
                    (None, Some(96), LineKind::Addition, "changed 96"),
                    (Some(97), Some(97), LineKind::Context, "line 97"),
                ],
            ),
        ],
        is_new: false,
        is_deleted: false,
    };
    let mut diff = DiffSet::with_contexts(vec![file], vec![Some(source)]);
    let collapsed =
        build_diff_row_data(&diff, DiffLayout::Unified, &HashSet::new(), &HashMap::new());
    let middle_gap = collapsed
        .context_gaps
        .iter()
        .find(|gap| gap.position == ContextGapPosition::Middle)
        .copied()
        .unwrap();
    assert_eq!(middle_gap.end - middle_gap.start, 88);
    assert!(matches!(middle_gap.source, ContextGapSource::File { .. }));

    let ContextGapSource::File {
        old_start,
        new_start,
    } = middle_gap.source
    else {
        unreachable!();
    };
    assert!(diff.insert_context(0, 0, false, old_start, new_start, 30));
    let expanded =
        build_diff_row_data(&diff, DiffLayout::Unified, &HashSet::new(), &HashMap::new());
    let remaining = expanded
        .context_gaps
        .iter()
        .find(|gap| gap.position == ContextGapPosition::Middle)
        .unwrap();
    assert_eq!(remaining.end - remaining.start, 58);
    let hunk = &diff.files[0].hunks[0];
    let last = hunk.lines.last().unwrap();
    assert_eq!((last.old_number, last.new_number), (Some(36), Some(36)));
    assert_eq!(hunk.line_content(last), "line 36");
    assert_eq!(expanded.rows.len(), collapsed.rows.len() + 30);
}

#[test]
fn file_tree_preserves_diff_order_between_sibling_entries() {
    let file = |path: &str| DiffFile {
        old_path: format!("a/{path}"),
        new_path: format!("b/{path}"),
        hunks: Vec::new(),
        is_new: false,
        is_deleted: false,
    };
    let files = vec![
        file("README.md"),
        file("src/z.rs"),
        file("src/a.rs"),
        file("docs/guide.md"),
    ];

    let rows = build_file_tree_rows(&files, &HashSet::new());
    assert!(matches!(
        rows.as_slice(),
        [
            FileTreeRow::File { file_index: 0, .. },
            FileTreeRow::Directory { path: src, .. },
            FileTreeRow::File { file_index: 1, .. },
            FileTreeRow::File { file_index: 2, .. },
            FileTreeRow::Directory { path: docs, .. },
            FileTreeRow::File { file_index: 3, .. },
        ] if src == "src" && docs == "docs"
    ));
}

#[test]
fn single_child_directory_chains_share_one_tree_row() {
    let file = |path: &str| DiffFile {
        old_path: format!("a/{path}"),
        new_path: format!("b/{path}"),
        hunks: Vec::new(),
        is_new: false,
        is_deleted: false,
    };
    let files = vec![
        file("src/features/auth/login.rs"),
        file("src/features/auth/logout.rs"),
    ];

    let expanded = build_file_tree_rows(&files, &HashSet::new());
    assert!(matches!(
        &expanded[0],
        FileTreeRow::Directory {
            path,
            name,
            depth: 0,
            expanded: true,
        } if path == "src/features/auth" && name == "src/features/auth"
    ));
    assert!(matches!(
        expanded[1],
        FileTreeRow::File {
            file_index: 0,
            depth: 1,
        }
    ));

    let collapsed = build_file_tree_rows(&files, &HashSet::from(["src/features/auth".to_owned()]));
    assert_eq!(collapsed.len(), 1);
    assert!(matches!(
        &collapsed[0],
        FileTreeRow::Directory {
            path,
            name,
            expanded: false,
            ..
        } if path == "src/features/auth" && name == "src/features/auth"
    ));
}

#[test]
fn scrolled_file_tree_keeps_the_current_directory_chain_sticky() {
    let file = |path: &str| DiffFile {
        old_path: format!("a/{path}"),
        new_path: format!("b/{path}"),
        hunks: Vec::new(),
        is_new: false,
        is_deleted: false,
    };
    let files = vec![
        file("src/components/button.rs"),
        file("src/lib.rs"),
        file("README.md"),
    ];
    let rows = build_file_tree_rows(&files, &HashSet::new());
    let offsets = file_tree_row_offsets(&rows);

    let nested = sticky_file_tree_directories(&rows, &offsets, px(70.));
    assert_eq!(nested.len(), 2);
    assert!(matches!(
        &nested[0],
        FileTreeRow::Directory { path, .. } if path == "src"
    ));
    assert!(matches!(
        &nested[1],
        FileTreeRow::Directory { path, .. } if path == "src/components"
    ));

    let source_root = sticky_file_tree_directories(&rows, &offsets, px(100.));
    assert_eq!(source_root.len(), 1);
    assert!(matches!(
        &source_root[0],
        FileTreeRow::Directory { path, .. } if path == "src"
    ));

    assert!(sticky_file_tree_directories(&rows, &offsets, px(120.)).is_empty());
}

#[test]
fn file_status_icons_cover_git_change_kinds() {
    let file = |old_path: &str, new_path: &str, is_new, is_deleted| DiffFile {
        old_path: old_path.into(),
        new_path: new_path.into(),
        hunks: Vec::new(),
        is_new,
        is_deleted,
    };

    assert_eq!(
        FileChangeKind::for_file(&file("a/main.rs", "b/main.rs", false, false)),
        FileChangeKind::Modified
    );
    assert_eq!(
        FileChangeKind::for_file(&file("/dev/null", "b/new.rs", true, false)),
        FileChangeKind::Added
    );
    assert_eq!(
        FileChangeKind::for_file(&file("a/old.rs", "/dev/null", false, true)),
        FileChangeKind::Deleted
    );
    assert_eq!(
        FileChangeKind::for_file(&file("a/old.rs", "b/new.rs", false, false)),
        FileChangeKind::Renamed
    );
    assert_eq!(FileChangeKind::Modified.label(), "M");
    assert_eq!(FileChangeKind::Added.label(), "A");
    assert_eq!(FileChangeKind::Deleted.label(), "D");
    assert_eq!(FileChangeKind::Renamed.label(), "R");
}

#[test]
fn inline_highlights_stay_on_utf8_boundaries() {
    let (old, new) = inline_ranges("let message = \"古い値\";", "let message = \"新しい値\";");
    let old_changed = old
        .into_iter()
        .map(|range| &"let message = \"古い値\";"[range])
        .collect::<String>();
    let new_changed = new
        .into_iter()
        .map(|range| &"let message = \"新しい値\";"[range])
        .collect::<String>();
    assert_eq!(old_changed, "古");
    assert_eq!(new_changed, "新し");
}

#[test]
fn inline_highlights_ignore_unrelated_replacement_lines() {
    let (old, new) = inline_ranges(
        "**Inline:** `::alert{type=\"warning\" icon=\"i-lucide-alert\"}`",
        "import { defineHandler, readBody, getQuery } from \"nitro/h3\";",
    );

    assert!(old.is_empty());
    assert!(new.is_empty());
}

#[test]
fn inline_highlights_group_changed_expressions() {
    let old = r#"<div v-for="(categoryExamples, category) in groupedExamples" :key="category">"#;
    let new = r#"<div v-for="group in groups" :key="group.category">"#;
    let (old_ranges, new_ranges) = inline_ranges(old, new);
    let old_changes = old_ranges
        .into_iter()
        .map(|range| &old[range])
        .collect::<Vec<_>>();
    let new_changes = new_ranges
        .into_iter()
        .map(|range| &new[range])
        .collect::<Vec<_>>();

    assert_eq!(
        old_changes,
        vec![
            "(categoryExamples, category) in groupedExamples",
            "category"
        ]
    );
    assert_eq!(new_changes, vec!["group in groups", "group.category"]);
}

#[test]
fn inline_highlights_refine_single_identifier_changes() {
    let old = "  <UPageBody>";
    let new = "  <PageBody>";
    let (old_ranges, new_ranges) = inline_ranges(old, new);

    assert_eq!(
        old_ranges
            .into_iter()
            .map(|range| &old[range])
            .collect::<Vec<_>>(),
        vec!["U"]
    );
    assert!(new_ranges.is_empty());
}

#[test]
fn unified_inline_diff_pairs_changed_lines_by_position() {
    let line = |kind, content: &str| (None, None, kind, content.to_owned());
    let hunk = DiffHunk::from_lines(
        "@@ -1,2 +1,3 @@",
        1,
        1,
        [
            line(LineKind::Deletion, "old zero"),
            line(LineKind::Deletion, "old one"),
            line(LineKind::Addition, "new zero"),
            line(LineKind::Addition, "new one"),
            line(LineKind::Addition, "new unpaired"),
        ],
    );

    assert_eq!(
        paired_line(&hunk, 0).map(|line| hunk.line_content(line)),
        Some("new zero")
    );
    assert_eq!(
        paired_line(&hunk, 1).map(|line| hunk.line_content(line)),
        Some("new one")
    );
    assert_eq!(
        paired_line(&hunk, 2).map(|line| hunk.line_content(line)),
        Some("old zero")
    );
    assert_eq!(
        paired_line(&hunk, 3).map(|line| hunk.line_content(line)),
        Some("old one")
    );
    assert!(paired_line(&hunk, 4).is_none());
}

#[test]
fn unified_row_index_resolves_paired_lines_from_compact_chunks() {
    let hunk = DiffHunk::from_lines(
        "@@ -1,2 +1,3 @@",
        1,
        1,
        [
            (Some(1), None, LineKind::Deletion, "old zero"),
            (Some(2), None, LineKind::Deletion, "old one"),
            (None, Some(1), LineKind::Addition, "new zero"),
            (None, Some(2), LineKind::Addition, "new one"),
            (None, Some(3), LineKind::Addition, "new unpaired"),
        ],
    );
    let file = DiffFile {
        old_path: "a/src/main.rs".into(),
        new_path: "b/src/main.rs".into(),
        hunks: vec![hunk],
        is_new: false,
        is_deleted: false,
    };
    let mut gaps = Vec::new();
    let rows = build_file_diff_rows(&file, 0, DiffLayout::Unified, &HashMap::new(), &mut gaps);
    let resolved = rows
        .iter()
        .map(|row| match row {
            DiffDisplayRow::Unified {
                row_index,
                counterpart_index,
                ..
            } => (row_index, unpack_diff_row_index(counterpart_index)),
            _ => panic!("expected unified row"),
        })
        .collect::<Vec<_>>();

    assert_eq!(
        resolved,
        vec![
            (0, Some(2)),
            (1, Some(3)),
            (2, Some(0)),
            (3, Some(1)),
            (4, None)
        ]
    );
    assert_eq!(rows.chunk_count(), 3);
}

#[test]
fn large_unified_change_uses_one_row_chunk() {
    let hunk = DiffHunk::from_lines(
        "@@ -0,0 +1,1000 @@",
        0,
        1,
        (1..=1000).map(|number| {
            (
                None,
                Some(number),
                LineKind::Addition,
                format!("line {number}"),
            )
        }),
    );
    let file = DiffFile {
        old_path: "/dev/null".into(),
        new_path: "b/generated.txt".into(),
        hunks: vec![hunk],
        is_new: true,
        is_deleted: false,
    };
    let mut gaps = Vec::new();
    let rows = build_file_diff_rows(&file, 0, DiffLayout::Unified, &HashMap::new(), &mut gaps);

    assert_eq!(rows.len(), 1000);
    assert_eq!(rows.chunk_count(), 1);
    assert!(matches!(
        rows.get(999),
        Some(DiffDisplayRow::Unified { row_index: 999, .. })
    ));
}

#[test]
fn large_split_change_uses_one_row_chunk() {
    let hunk = DiffHunk::from_lines(
        "@@ -0,0 +1,1000 @@",
        0,
        1,
        (1..=1000).map(|number| {
            (
                None,
                Some(number),
                LineKind::Addition,
                format!("line {number}"),
            )
        }),
    );
    let file = DiffFile {
        old_path: "/dev/null".into(),
        new_path: "b/generated.txt".into(),
        hunks: vec![hunk],
        is_new: true,
        is_deleted: false,
    };
    let mut gaps = Vec::new();
    let rows = build_file_diff_rows(&file, 0, DiffLayout::Split, &HashMap::new(), &mut gaps);

    assert_eq!(rows.len(), 1000);
    assert_eq!(rows.chunk_count(), 1);
    assert!(matches!(
        rows.get(999),
        Some(DiffDisplayRow::Split {
            old_line_index: super::NO_DIFF_ROW_INDEX,
            new_line_index: 999,
            ..
        })
    ));
}

#[test]
fn syntect_produces_multiple_styles_for_rust() {
    let highlights = syntax_highlights(
        "pub fn answer() -> usize { 42 }",
        "Rust",
        Palette::for_mode(ThemeMode::Dark),
    );
    assert!(highlights.len() >= 3);
}
