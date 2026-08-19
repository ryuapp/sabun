use gpui::{PathBuilder, Pixels, Point, Rgba, Window, point, px};

use super::ExpandIconDirection;

pub(super) fn paint(
    center: Point<Pixels>,
    direction: ExpandIconDirection,
    color: Rgba,
    size: Pixels,
    window: &mut Window,
) {
    let scale = f32::from(size) / 14.;
    let sign = match direction {
        ExpandIconDirection::Up => -1.,
        ExpandIconDirection::Down => 1.,
    };
    let mut path = PathBuilder::stroke(px(scale));
    path.move_to(center + point(px(-4. * scale), px(-6. * sign * scale)));
    path.line_to(center + point(px(4. * scale), px(-6. * sign * scale)));
    path.move_to(center + point(px(0.), px(-4. * sign * scale)));
    path.line_to(center + point(px(0.), px(2. * sign * scale)));
    path.move_to(center + point(px(-3. * scale), px(-sign * scale)));
    path.line_to(center + point(px(0.), px(2. * sign * scale)));
    path.line_to(center + point(px(3. * scale), px(-sign * scale)));
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}
