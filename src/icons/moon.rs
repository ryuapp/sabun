use gpui::{PathBuilder, Pixels, Point, Rgba, Window, point, px};

pub(super) fn paint(center: Point<Pixels>, color: Rgba, window: &mut Window) {
    let top = center + point(px(2.8), px(-7.));
    let bottom = center + point(px(2.8), px(7.));
    let mut moon = PathBuilder::fill();
    moon.move_to(top);
    moon.cubic_bezier_to(
        bottom,
        center + point(px(-5.6), px(-5.8)),
        center + point(px(-5.6), px(5.8)),
    );
    moon.cubic_bezier_to(
        top,
        center + point(px(-0.2), px(4.5)),
        center + point(px(-0.2), px(-4.5)),
    );
    moon.close();
    if let Ok(path) = moon.build() {
        window.paint_path(path, color);
    }
}
