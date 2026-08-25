#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThemeMode {
    Dark,
    Light,
}

pub struct AppTheme {
    textmate_json: &'static str,
}

impl AppTheme {
    const fn new(textmate_json: &'static str) -> Self {
        Self { textmate_json }
    }

    #[must_use]
    pub const fn textmate_json(&self) -> &'static str {
        self.textmate_json
    }
}

pub static GITHUB_DARK: AppTheme = AppTheme::new(include_str!("github-dark.json"));
pub static GITHUB_LIGHT: AppTheme = AppTheme::new(include_str!("github-light.json"));
pub static SABUN_DARK: AppTheme = AppTheme::new(include_str!("sabun-dark.json"));

#[must_use]
pub fn app_theme(mode: ThemeMode) -> &'static AppTheme {
    match mode {
        ThemeMode::Dark => &SABUN_DARK,
        ThemeMode::Light => &GITHUB_LIGHT,
    }
}
