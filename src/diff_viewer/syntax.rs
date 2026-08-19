use super::{
    DiffFile, FontStyle, FontWeight, HighlightLines, HighlightStyle, OnceLock, Palette, Path,
    Range, Rgba, SyntaxSet, SyntectFontStyle, ThemeMode, rgb,
};
use syntect::{
    highlighting::{Color, Highlighter, Theme, ThemeSet},
    parsing::Scope,
};

#[derive(Clone, Copy)]
pub(super) struct SyntaxDiffColors {
    pub(super) addition: Option<Rgba>,
    pub(super) addition_background: Option<Rgba>,
    pub(super) deletion: Option<Rgba>,
    pub(super) deletion_background: Option<Rgba>,
}

pub(super) fn language_color(language: &str) -> Rgba {
    match language {
        "Rust" => rgb(0xe48a63),
        "TypeScript" | "TypescriptReact" => rgb(0x5a9ee6),
        "JavaScript" | "JavaScript (Babel)" => rgb(0xe8c95b),
        "Python" => rgb(0x6ea7d7),
        "Markdown" => rgb(0x8b7cf6),
        "Go" => rgb(0x55bfc8),
        "Vue" | "Vue Component" => rgb(0x42b883),
        "Svelte" => rgb(0xff6b35),
        "HTML" => rgb(0xe76f51),
        "CSS" | "SCSS" | "Sass" | "LESS" => rgb(0x6c8cff),
        "Kotlin" | "Swift" => rgb(0xb785f5),
        "C" | "C++" | "C#" => rgb(0x7d9bd1),
        _ => rgb(0x8b97a8),
    }
}

pub(super) fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(two_face::syntax::extra_newlines)
}

pub(super) fn detected_language(path: &str) -> &'static str {
    let syntax_set = syntax_set();
    let path = Path::new(path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    let lowercase_name = file_name.to_ascii_lowercase();
    let lowercase_extension = extension.to_ascii_lowercase();

    syntax_set
        .find_syntax_by_extension(file_name)
        .or_else(|| syntax_set.find_syntax_by_extension(&lowercase_name))
        .or_else(|| syntax_set.find_syntax_by_extension(extension))
        .or_else(|| syntax_set.find_syntax_by_extension(&lowercase_extension))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text())
        .name
        .as_str()
}

pub(super) fn detected_language_for_file(file: &DiffFile) -> &'static str {
    let path_language = detected_language(file.display_path());
    if path_language != "Plain Text" {
        return path_language;
    }

    file.hunks
        .iter()
        .find_map(|hunk| {
            hunk.lines
                .iter()
                .find(|line| line.old_number == Some(1) || line.new_number == Some(1))
                .map(|line| hunk.line_content(line))
        })
        .and_then(|line| syntax_set().find_syntax_by_first_line(line))
        .unwrap_or_else(|| syntax_set().find_syntax_plain_text())
        .name
        .as_str()
}

pub(super) fn trim_diff_prefix(path: &str) -> String {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .to_owned()
}

pub(super) fn syntax_highlights(
    content: &str,
    language: &str,
    palette: Palette,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let highlights = syntax_highlights_for_language(content, language, palette);
    if highlights.len() > 1 {
        return highlights;
    }

    embedded_language(language, content)
        .map(|embedded| syntax_highlights_for_language(content, embedded, palette))
        .filter(|embedded| embedded.len() > highlights.len())
        .unwrap_or(highlights)
}

pub(super) fn syntax_highlighter(language: &str, theme: ThemeMode) -> HighlightLines<'static> {
    let syntax_set = syntax_set();
    let syntax = syntax_set
        .find_syntax_by_name(language)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    HighlightLines::new(syntax, syntax_theme(theme))
}

pub(super) fn syntax_highlights_with_state(
    highlighter: &mut HighlightLines<'static>,
    embedded_highlighter: &mut Option<(&'static str, HighlightLines<'static>)>,
    line_buffer: &mut String,
    content: &str,
    language: &str,
    palette: Palette,
) -> Vec<(Range<usize>, HighlightStyle)> {
    line_buffer.clear();
    line_buffer.push_str(content);
    line_buffer.push('\n');
    let highlights = highlight_prepared_line(highlighter, line_buffer, content.len());
    if highlights.len() > 1 {
        *embedded_highlighter = None;
        return highlights;
    }

    let Some(embedded) = embedded_language(language, content) else {
        *embedded_highlighter = None;
        return highlights;
    };
    if embedded_highlighter
        .as_ref()
        .is_none_or(|(language, _)| *language != embedded)
    {
        *embedded_highlighter = Some((embedded, syntax_highlighter(embedded, palette.theme())));
    }
    let embedded_highlights = embedded_highlighter
        .as_mut()
        .map_or_else(Vec::new, |(_, highlighter)| {
            highlight_prepared_line(highlighter, line_buffer, content.len())
        });
    if embedded_highlights.len() > highlights.len() {
        embedded_highlights
    } else {
        highlights
    }
}

pub(super) fn embedded_language(language: &str, content: &str) -> Option<&'static str> {
    let trimmed = content.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('<')
        || trimmed.contains("{{")
        || trimmed.starts_with("{#")
        || trimmed.starts_with("{/")
    {
        return None;
    }
    let looks_like_css = trimmed.starts_with('.')
        || trimmed.starts_with('#')
        || trimmed.starts_with('@')
        || trimmed.starts_with("--")
        || (trimmed.contains(':')
            && (trimmed.ends_with(';') || trimmed.ends_with('{') || trimmed.ends_with('}')));

    match language {
        "Vue Component" => Some(if looks_like_css { "CSS" } else { "TypeScript" }),
        "Svelte" | "HTML" => Some(if looks_like_css { "CSS" } else { "JavaScript" }),
        _ => None,
    }
}

pub(super) fn syntax_highlights_for_language(
    content: &str,
    language: &str,
    palette: Palette,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let syntax_set = syntax_set();
    let syntax = syntax_set
        .find_syntax_by_name(language)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, syntax_theme(palette.theme()));
    highlight_line(&mut highlighter, content)
}

pub(super) fn syntax_theme(theme: ThemeMode) -> &'static syntect::highlighting::Theme {
    static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

    let theme_name = match theme {
        ThemeMode::Dark => "base16-eighties.dark",
        ThemeMode::Light => "InspiredGitHub",
    };
    THEME_SET
        .get_or_init(ThemeSet::load_defaults)
        .themes
        .get(theme_name)
        .expect("bundled syntect theme")
}

pub(super) fn syntax_diff_colors(mode: ThemeMode) -> SyntaxDiffColors {
    static LIGHT: OnceLock<SyntaxDiffColors> = OnceLock::new();
    static DARK: OnceLock<SyntaxDiffColors> = OnceLock::new();
    let colors = match mode {
        ThemeMode::Light => &LIGHT,
        ThemeMode::Dark => &DARK,
    };
    *colors.get_or_init(|| syntax_diff_colors_from_theme(syntax_theme(mode)))
}

fn syntax_diff_colors_from_theme(theme: &Theme) -> SyntaxDiffColors {
    let (addition, addition_background) = colors_for_scope(theme, "markup.inserted");
    let (deletion, deletion_background) = colors_for_scope(theme, "markup.deleted");
    SyntaxDiffColors {
        addition: addition.map(rgba_from_syntect),
        addition_background: addition_background.map(rgba_from_syntect),
        deletion: deletion.map(rgba_from_syntect),
        deletion_background: deletion_background.map(rgba_from_syntect),
    }
}

fn colors_for_scope(theme: &Theme, scope: &str) -> (Option<Color>, Option<Color>) {
    let scope = Scope::new(scope).expect("static syntax scope");
    let style = Highlighter::new(theme).style_mod_for_stack(&[scope]);
    (style.foreground, style.background)
}

fn rgba_from_syntect(color: Color) -> Rgba {
    Rgba {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: f32::from(color.a) / 255.0,
    }
}

fn highlight_line(
    highlighter: &mut HighlightLines<'_>,
    content: &str,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let line = format!("{content}\n");
    highlight_prepared_line(highlighter, &line, content.len())
}

fn highlight_prepared_line(
    highlighter: &mut HighlightLines<'_>,
    line: &str,
    content_len: usize,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let Ok(regions) = highlighter.highlight_line(line, syntax_set()) else {
        return Vec::new();
    };

    let mut offset = 0;
    let mut highlights = Vec::with_capacity(regions.len());
    for (style, text) in regions {
        let start = offset.min(content_len);
        offset += text.len();
        let end = offset.min(content_len);
        if start == end {
            continue;
        }
        highlights.push((
            start..end,
            HighlightStyle {
                color: Some(rgba_from_syntect(style.foreground).into()),
                font_weight: style
                    .font_style
                    .contains(SyntectFontStyle::BOLD)
                    .then_some(FontWeight::BOLD),
                font_style: style
                    .font_style
                    .contains(SyntectFontStyle::ITALIC)
                    .then_some(FontStyle::Italic),
                ..Default::default()
            },
        ));
    }
    highlights
}
