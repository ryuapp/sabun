mod check;
mod copy;
mod disclosure;
mod expand;
mod moon;
mod sun;

use gpui::{IntoElement, Pixels, Rgba, Styled, canvas, px};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeIcon {
    Moon,
    Sun,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpandIconDirection {
    Up,
    Down,
}

pub const DISCLOSURE_ICON_SIZE: f32 = 18.;

pub fn disclosure_icon(expanded: bool, color: Rgba) -> impl IntoElement {
    let size = px(DISCLOSURE_ICON_SIZE);
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            disclosure::paint(bounds.center(), expanded, size, color, window);
        },
    )
    .size(size)
}

pub fn copy_icon(color: Rgba, size: Pixels) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            copy::paint(bounds.center(), color, size, window);
        },
    )
    .size(size)
}

pub fn check_icon(color: Rgba, size: Pixels) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            check::paint(bounds.center(), color, size, window);
        },
    )
    .size(size)
}

pub fn context_expand_icon(
    direction: ExpandIconDirection,
    color: Rgba,
    size: Pixels,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            expand::paint(bounds.center(), direction, color, size, window);
        },
    )
    .size(size)
}

pub fn theme_icon(icon: ThemeIcon, color: Rgba) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let center = bounds.center();
            match icon {
                ThemeIcon::Moon => moon::paint(center, color, window),
                ThemeIcon::Sun => sun::paint(center, color, window),
            }
        },
    )
    .size(px(18.))
}
