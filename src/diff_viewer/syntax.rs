use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    ops::Range,
    path::Path,
    sync::{Mutex, OnceLock},
};

use syntaxmate::{
    CheckpointTable, DocumentLine, FontModifiers, HighlightScopeTable, PreparedLanguage, RgbColor,
    ScopedToken, Style, TextMateTheme as SyntaxTheme, Tokenizer, TokenizerOptions, TokenizerState,
    available_languages, canonical_language, detect_language_from_path,
};

use super::{
    DiffFile, FontStyle, FontWeight, HighlightStyle, Palette, Rgba, ThemeMode, app_theme, rgb,
};

const PLAIN_TEXT: &str = "plaintext";
const MAX_STYLE_CACHE_ENTRIES: usize = 1_024;

pub(super) struct SyntaxHighlighter {
    session: Option<PreparedHighlightSession>,
}

struct PreparedHighlightSession {
    tokenizer: Tokenizer,
    state: TokenizerState,
    checkpoints: CheckpointTable,
    theme: SyntaxTheme,
    tokens: Vec<ScopedToken>,
    style_cache: StyleCache,
}

#[derive(Default)]
struct StyleCache {
    entries: HashMap<u64, Vec<CachedStyle>>,
    len: usize,
}

struct CachedStyle {
    scopes: Box<[Box<str>]>,
    style: Style,
}

impl CachedStyle {
    fn matches(&self, token: &ScopedToken) -> bool {
        self.scopes.iter().map(AsRef::as_ref).eq(token.scopes())
    }
}

impl StyleCache {
    fn resolve(&mut self, token: &ScopedToken, theme: &SyntaxTheme) -> Style {
        let mut hasher = DefaultHasher::new();
        for scope in token.scopes() {
            scope.hash(&mut hasher);
        }
        let hash = hasher.finish();

        if let Some(style) = self
            .entries
            .get(&hash)
            .and_then(|entries| entries.iter().find(|entry| entry.matches(token)))
            .map(|entry| entry.style)
        {
            return style;
        }

        let scopes = token.scopes().collect::<Vec<_>>();
        let (scope_table, scope_stack) = HighlightScopeTable::from_scope_names(&scopes);
        let style = theme.resolve(&scope_table, scope_stack);

        // SyntaxMate's public token API does not expose its internal scope-stack ID. Hashing the
        // names lets repeated tokens avoid rebuilding the theme lookup table, while the exact
        // comparison above keeps hash collisions harmless. Keep the cache bounded like
        // SyntaxMate's own HighlightSession cache.
        if self.len < MAX_STYLE_CACHE_ENTRIES {
            self.entries.entry(hash).or_default().push(CachedStyle {
                scopes: scopes.into_iter().map(Into::into).collect(),
                style,
            });
            self.len += 1;
        }

        style
    }
}

impl PreparedHighlightSession {
    fn new(language: &'static str, theme: ThemeMode) -> Option<Self> {
        let prepared = prepared_language(language)?;
        let tokenizer = prepared.tokenizer(TokenizerOptions::default());
        let state = tokenizer.initial_state();
        let checkpoints = tokenizer.checkpoints(128);
        Some(Self {
            tokenizer,
            state,
            checkpoints,
            theme: app_theme(theme).syntax().clone(),
            tokens: Vec::new(),
            style_cache: StyleCache::default(),
        })
    }

    fn highlight_line(&mut self, content: &str) -> Option<Vec<(Range<usize>, HighlightStyle)>> {
        self.tokenizer
            .tokenize_line_into(content, &mut self.state, &mut self.tokens)
            .ok()?;

        Some(
            self.tokens
                .iter()
                .filter_map(|token| {
                    let range = token.range();
                    (!range.is_empty() && range.end <= content.len()).then(|| {
                        let style = self.style_cache.resolve(token, &self.theme);
                        (range, highlight_style(style))
                    })
                })
                .collect(),
        )
    }

    fn highlight_source_line(
        &mut self,
        source: &str,
        line_index: usize,
    ) -> Option<Vec<(Range<usize>, HighlightStyle)>> {
        // Virtualized rows can be requested in any order. SyntaxMate replays from the nearest
        // checkpoint, so a direct jump still sees frontmatter, comments, and embedded languages
        // opened above the viewport without eagerly styling the complete file.
        let document = self
            .tokenizer
            .tokenize_viewport(
                source,
                line_index..line_index.checked_add(1)?,
                &mut self.checkpoints,
            )
            .ok()?;
        document
            .lines()
            .first()
            .map(|line| document_line_highlights(line, &self.theme))
    }
}

impl SyntaxHighlighter {
    fn highlight_line(&mut self, content: &str) -> Vec<(Range<usize>, HighlightStyle)> {
        let Some(session) = &mut self.session else {
            return Vec::new();
        };
        session.highlight_line(content).unwrap_or_default()
    }

    pub(super) fn highlight_source_line(
        &mut self,
        source: &str,
        line_number: u32,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let Some(line_index) = line_number
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
        else {
            return Vec::new();
        };
        self.session
            .as_mut()
            .and_then(|session| session.highlight_source_line(source, line_index))
            .unwrap_or_default()
    }
}

fn document_line_highlights(
    line: &DocumentLine,
    theme: &SyntaxTheme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    line.spans()
        .iter()
        .filter_map(|span| {
            let range = span.range();
            (!range.is_empty()).then(|| {
                let style = theme.resolve(line.scope_table(), span.scope_stack());
                (range, highlight_style(style))
            })
        })
        .collect()
}

fn prepared_language(language: &'static str) -> Option<PreparedLanguage> {
    static LANGUAGES: OnceLock<Mutex<HashMap<&'static str, PreparedLanguage>>> = OnceLock::new();

    let languages = LANGUAGES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut languages = languages.lock().ok()?;
    if let Some(prepared) = languages.get(language) {
        return Some(prepared.clone());
    }

    // A diff owns many independent streams (old/new sides and separate hunks). PreparedLanguage
    // shares their immutable grammar graph, compiled regexes, and candidate caches; each stream
    // still gets its own TokenizerState, so multiline constructs cannot leak into another hunk.
    // HighlightSession currently prepares its bundled grammar internally, hence this small local
    // session built from SyntaxMate's lower-level public API.
    let prepared = PreparedLanguage::for_bundled_language(language).ok()?;
    languages.insert(language, prepared.clone());
    drop(languages);
    Some(prepared)
}

pub(super) fn language_color(language: &str) -> Rgba {
    match language {
        "rust" => rgb(0xe48a63),
        "typescript" | "tsx" => rgb(0x5a9ee6),
        "javascript" | "jsx" => rgb(0xe8c95b),
        "python" => rgb(0x6ea7d7),
        "markdown" | "mdx" => rgb(0x8b7cf6),
        "go" => rgb(0x55bfc8),
        "vue" => rgb(0x42b883),
        "svelte" => rgb(0xff6b35),
        "html" => rgb(0xe76f51),
        "css" | "scss" | "sass" | "less" => rgb(0x6c8cff),
        "kotlin" | "swift" => rgb(0xb785f5),
        "c" | "cpp" | "csharp" => rgb(0x7d9bd1),
        _ => rgb(0x8b97a8),
    }
}

fn syntax_languages() -> &'static [Box<str>] {
    static LANGUAGES: OnceLock<Vec<Box<str>>> = OnceLock::new();
    LANGUAGES.get_or_init(|| {
        available_languages()
            .into_iter()
            .map(String::into_boxed_str)
            .collect()
    })
}

fn language_id(language: &str) -> Option<&'static str> {
    let canonical = canonical_language(language)
        .or_else(|| canonical_language(&language.to_ascii_lowercase()))?;
    syntax_languages()
        .iter()
        .find(|candidate| candidate.as_ref() == canonical)
        .map(AsRef::as_ref)
}

#[cfg(test)]
pub(super) fn syntax_language_count() -> usize {
    syntax_languages().len()
}

pub(super) fn detected_language(path: &str) -> &'static str {
    detect_language_from_path(Path::new(path))
        .as_deref()
        .and_then(language_id)
        .unwrap_or(PLAIN_TEXT)
}

pub(super) fn detected_language_for_file(file: &DiffFile) -> &'static str {
    let path_language = detected_language(file.display_path());
    if path_language != PLAIN_TEXT {
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
        .and_then(language_from_shebang)
        .unwrap_or(PLAIN_TEXT)
}

fn language_from_shebang(line: &str) -> Option<&'static str> {
    let command = line.trim().strip_prefix("#!")?.trim();
    let mut arguments = command.split_whitespace();
    let executable = arguments.next()?;
    let executable = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())?;
    let interpreter = if executable == "env" {
        arguments.find(|argument| !argument.starts_with('-') && !argument.contains('='))?
    } else {
        executable
    };
    let interpreter = Path::new(interpreter)
        .file_name()
        .and_then(|name| name.to_str())?
        .to_ascii_lowercase();
    let language = if interpreter.starts_with("python") {
        "python"
    } else if matches!(interpreter.as_str(), "node" | "nodejs" | "deno" | "bun") {
        "javascript"
    } else if matches!(
        interpreter.as_str(),
        "sh" | "bash" | "dash" | "zsh" | "fish"
    ) {
        "shellscript"
    } else if interpreter.starts_with("ruby") {
        "ruby"
    } else if interpreter.starts_with("perl") {
        "perl"
    } else if interpreter.starts_with("php") {
        "php"
    } else if interpreter.starts_with("lua") {
        "lua"
    } else if matches!(interpreter.as_str(), "pwsh" | "powershell") {
        "powershell"
    } else {
        interpreter.as_str()
    };
    language_id(language)
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
    let highlights = syntax_highlights_for_language(content, language, palette.theme());
    if highlights.len() > 1 {
        return highlights;
    }

    embedded_language(language, content)
        .map(|embedded| syntax_highlights_for_language(content, embedded, palette.theme()))
        .filter(|embedded| embedded.len() > highlights.len())
        .unwrap_or(highlights)
}

pub(super) fn syntax_highlighter(language: &str, theme: ThemeMode) -> SyntaxHighlighter {
    let session =
        language_id(language).and_then(|language| PreparedHighlightSession::new(language, theme));
    SyntaxHighlighter { session }
}

pub(super) fn syntax_highlights_with_state(
    highlighter: &mut SyntaxHighlighter,
    embedded_highlighter: &mut Option<(&'static str, SyntaxHighlighter)>,
    content: &str,
    language: &str,
    palette: Palette,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let highlights = highlighter.highlight_line(content);
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
            highlighter.highlight_line(content)
        });
    if embedded_highlights.len() > highlights.len() {
        embedded_highlights
    } else {
        highlights
    }
}

fn embedded_language(language: &str, content: &str) -> Option<&'static str> {
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
        "vue" => Some(if looks_like_css { "css" } else { "typescript" }),
        "svelte" | "html" => Some(if looks_like_css { "css" } else { "javascript" }),
        _ => None,
    }
}

fn syntax_highlights_for_language(
    content: &str,
    language: &str,
    theme: ThemeMode,
) -> Vec<(Range<usize>, HighlightStyle)> {
    syntax_highlighter(language, theme).highlight_line(content)
}

fn highlight_style(style: Style) -> HighlightStyle {
    HighlightStyle {
        color: style.foreground.map(rgba_from_syntaxmate).map(Into::into),
        font_weight: style
            .modifiers
            .contains(FontModifiers::BOLD)
            .then_some(FontWeight::BOLD),
        font_style: style
            .modifiers
            .contains(FontModifiers::ITALIC)
            .then_some(FontStyle::Italic),
        ..Default::default()
    }
}

fn rgba_from_syntaxmate(color: RgbColor) -> Rgba {
    Rgba {
        r: f32::from(color.red) / 255.0,
        g: f32::from(color.green) / 255.0,
        b: f32::from(color.blue) / 255.0,
        a: 1.0,
    }
}
