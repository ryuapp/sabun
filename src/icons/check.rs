use gpui::{PathBuilder, Pixels, Point, Rgba, Window, point, px};

pub(super) fn paint(center: Point<Pixels>, color: Rgba, size: Pixels, window: &mut Window) {
    let scale = f32::from(size) / 16.;
    let mut path = PathBuilder::stroke(px(1.7 * scale));
    path.move_to(center + point(px(-5. * scale), px(0.5 * scale)));
    path.line_to(center + point(px(-1.5 * scale), px(4. * scale)));
    path.line_to(center + point(px(6. * scale), px(-5. * scale)));
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}
