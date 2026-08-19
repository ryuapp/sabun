use gpui::{PathBuilder, Pixels, Point, Rgba, Window, point, px};

pub(super) fn paint(center: Point<Pixels>, color: Rgba, window: &mut Window) {
    let radius = px(3.25);
    let mut disc = PathBuilder::fill();
    disc.move_to(center + point(radius, px(0.)));
    disc.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        center + point(-radius, px(0.)),
    );
    disc.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        center + point(radius, px(0.)),
    );
    disc.close();
    if let Ok(path) = disc.build() {
        window.paint_path(path, color);
    }

    let mut rays = PathBuilder::stroke(px(1.4));
    for (from, to) in [
        (point(px(0.), px(-5.5)), point(px(0.), px(-7.5))),
        (point(px(0.), px(5.5)), point(px(0.), px(7.5))),
        (point(px(-5.5), px(0.)), point(px(-7.5), px(0.))),
        (point(px(5.5), px(0.)), point(px(7.5), px(0.))),
        (point(px(-4.), px(-4.)), point(px(-5.4), px(-5.4))),
        (point(px(4.), px(-4.)), point(px(5.4), px(-5.4))),
        (point(px(-4.), px(4.)), point(px(-5.4), px(5.4))),
        (point(px(4.), px(4.)), point(px(5.4), px(5.4))),
    ] {
        rays.move_to(center + from);
        rays.line_to(center + to);
    }
    if let Ok(path) = rays.build() {
        window.paint_path(path, color);
    }
}
