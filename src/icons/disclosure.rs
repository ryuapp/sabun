use gpui::{PathBuilder, Pixels, Point, Rgba, Window, point, px};

pub(super) fn paint(
    center: Point<Pixels>,
    expanded: bool,
    size: Pixels,
    color: Rgba,
    window: &mut Window,
) {
    let scale = f32::from(size) / super::DISCLOSURE_ICON_SIZE;
    let mut chevron = PathBuilder::stroke(px(1.5 * scale));

    if expanded {
        chevron.move_to(center + point(px(-5. * scale), px(-3. * scale)));
        chevron.line_to(center + point(px(0.), px(2. * scale)));
        chevron.line_to(center + point(px(5. * scale), px(-3. * scale)));
    } else {
        chevron.move_to(center + point(px(-3. * scale), px(-5. * scale)));
        chevron.line_to(center + point(px(2. * scale), px(0.)));
        chevron.line_to(center + point(px(-3. * scale), px(5. * scale)));
    }

    if let Ok(path) = chevron.build() {
        window.paint_path(path, color);
    }
}
