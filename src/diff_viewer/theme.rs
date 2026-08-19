use super::{Rgba, rgb};

const INLINE_THEME_BLEND: f32 = 0.2;

pub(super) const fn with_alpha(mut color: Rgba, alpha: f32) -> Rgba {
    color.a = alpha;
    color
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ThemeMode {
    Dark,
    Light,
}

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

    pub(super) fn for_mode(mode: ThemeMode) -> Self {
        let mut palette = match mode {
            ThemeMode::Dark => Self {
                canvas: rgb(0x181818),
                sidebar: rgb(0x181818),
                panel: rgb(0x181818),
                elevated: rgb(0x181818),
                hover: rgb(0x292929),
                border: rgb(0x333333),
                text: rgb(0xededed),
                muted: rgb(0xaaaaaa),
                faint: rgb(0x707070),
                accent: rgb(0x8b7cf6),
                green: rgb(0x57d89b),
                green_bg: rgb(0x102b24),
                green_inline: rgb(0x19543f),
                yellow: rgb(0xd9ad62),
                red: rgb(0xff7b88),
                red_bg: rgb(0x321b22),
                red_inline: rgb(0x672735),
                blue: rgb(0x72b7ff),
                selection: rgb(0x3a3a3a),
            },
            ThemeMode::Light => Self {
                canvas: rgb(0xffffff),
                sidebar: rgb(0xffffff),
                panel: rgb(0xffffff),
                elevated: rgb(0xffffff),
                hover: rgb(0xf2f2f2),
                border: rgb(0xe5e5e5),
                text: rgb(0x1c1c1c),
                muted: rgb(0x606060),
                faint: rgb(0x858585),
                accent: rgb(0x6757d9),
                green: rgb(0x16855b),
                green_bg: rgb(0xe7f7ef),
                green_inline: rgb(0xbcebd2),
                yellow: rgb(0x9a6700),
                red: rgb(0xc83f54),
                red_bg: rgb(0xffedf0),
                red_inline: rgb(0xf7c3cc),
                blue: rgb(0x2878c7),
                selection: rgb(0xe7e7e7),
            },
        };
        let diff = super::syntax::syntax_diff_colors(mode);
        let addition_changed = diff.addition.is_some() || diff.addition_background.is_some();
        let deletion_changed = diff.deletion.is_some() || diff.deletion_background.is_some();
        if let Some(color) = diff.addition {
            palette.green = color;
        }
        if let Some(color) = diff.addition_background {
            palette.green_bg = color;
        }
        if addition_changed {
            palette.green_inline = blend(palette.green_bg, palette.green, INLINE_THEME_BLEND);
        }
        if let Some(color) = diff.deletion {
            palette.red = color;
        }
        if let Some(color) = diff.deletion_background {
            palette.red_bg = color;
        }
        if deletion_changed {
            palette.red_inline = blend(palette.red_bg, palette.red, INLINE_THEME_BLEND);
        }
        palette
    }
}

fn blend(base: Rgba, accent: Rgba, amount: f32) -> Rgba {
    let inverse = 1. - amount;
    Rgba {
        r: accent.r.mul_add(amount, base.r * inverse),
        g: accent.g.mul_add(amount, base.g * inverse),
        b: accent.b.mul_add(amount, base.b * inverse),
        a: accent.a.mul_add(amount, base.a * inverse),
    }
}
