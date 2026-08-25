use std::sync::OnceLock;

use gpui::Rgba;
use syntaxmate::{RgbColor, TextMateTheme as SyntaxTheme};
pub(super) use themes::ThemeMode;

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub(super) canvas: Rgba,
    pub(super) sidebar: Rgba,
    pub(super) panel: Rgba,
    pub(super) elevated: Rgba,
    pub(super) hover: Rgba,
    pub(super) border: Rgba,
    pub(super) text: Rgba,
    pub(super) muted: Rgba,
    pub(super) faint: Rgba,
    pub(super) accent: Rgba,
    pub(super) green: Rgba,
    pub(super) green_bg: Rgba,
    pub(super) green_inline: Rgba,
    pub(super) yellow: Rgba,
    pub(super) red: Rgba,
    pub(super) red_bg: Rgba,
    pub(super) red_inline: Rgba,
    pub(super) blue: Rgba,
    pub(super) selection: Rgba,
}

impl Palette {
    pub(super) const fn theme(self) -> ThemeMode {
        if self.canvas.r < 0.5 {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        }
    }
}

impl Palette {
    fn from_theme(theme: &SyntaxTheme) -> Self {
        Self {
            canvas: theme_color(theme, "editor.background"),
            sidebar: theme_color(theme, "sideBar.background"),
            panel: theme_color(theme, "panel.background"),
            elevated: theme_color(theme, "editorWidget.background"),
            hover: theme_color(theme, "list.hoverBackground"),
            border: theme_color(theme, "panel.border"),
            text: theme_color(theme, "editor.foreground"),
            muted: theme_color(theme, "descriptionForeground"),
            faint: theme_color(theme, "editorLineNumber.foreground"),
            accent: theme_color(theme, "focusBorder"),
            green: theme_color(theme, "gitDecoration.addedResourceForeground"),
            green_bg: theme_color(theme, "diffEditor.insertedLineBackground"),
            green_inline: theme_color(theme, "diffEditor.insertedTextBackground"),
            yellow: theme_color(theme, "gitDecoration.modifiedResourceForeground"),
            red: theme_color(theme, "gitDecoration.deletedResourceForeground"),
            red_bg: theme_color(theme, "diffEditor.removedLineBackground"),
            red_inline: theme_color(theme, "diffEditor.removedTextBackground"),
            blue: theme_color(theme, "terminal.ansiBlue"),
            selection: theme_color(theme, "editor.selectionBackground"),
        }
    }
}

pub(super) const fn with_alpha(mut color: Rgba, alpha: f32) -> Rgba {
    color.a = alpha;
    color
}

pub(super) struct AppTheme {
    syntax: SyntaxTheme,
    palette: Palette,
}

impl AppTheme {
    fn from_definition(definition: &'static themes::AppTheme) -> Self {
        // Use SyntaxMate's lower-level theme type so token styles and UI colors are parsed once
        // from the same portable TextMate JSON instead of maintaining a parallel Rust palette.
        let syntax = SyntaxTheme::from_json(definition.textmate_json()).expect("valid app theme");
        let palette = Palette::from_theme(&syntax);
        Self { syntax, palette }
    }

    pub(super) const fn syntax(&self) -> &SyntaxTheme {
        &self.syntax
    }

    pub(super) const fn palette(&self) -> Palette {
        self.palette
    }
}

pub(super) fn app_theme(mode: ThemeMode) -> &'static AppTheme {
    static DARK: OnceLock<AppTheme> = OnceLock::new();
    static LIGHT: OnceLock<AppTheme> = OnceLock::new();
    match mode {
        ThemeMode::Dark => DARK.get_or_init(|| AppTheme::from_definition(themes::app_theme(mode))),
        ThemeMode::Light => {
            LIGHT.get_or_init(|| AppTheme::from_definition(themes::app_theme(mode)))
        }
    }
}

fn theme_color(theme: &SyntaxTheme, name: &str) -> Rgba {
    theme.color(name).map_or_else(
        || panic!("app theme is missing `{name}`"),
        rgba_from_syntaxmate,
    )
}

fn rgba_from_syntaxmate(color: RgbColor) -> Rgba {
    Rgba {
        r: f32::from(color.red) / 255.0,
        g: f32::from(color.green) / 255.0,
        b: f32::from(color.blue) / 255.0,
        a: 1.0,
    }
}
