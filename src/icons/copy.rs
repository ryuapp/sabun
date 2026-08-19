use gpui::{PathBuilder, Pixels, Point, Rgba, Window, point, px};

pub(super) fn paint(center: Point<Pixels>, color: Rgba, size: Pixels, window: &mut Window) {
    let scale = f32::from(size) / 16.;
    let stroke = px(1.25 * scale);
    let mut path = PathBuilder::stroke(stroke);

    let rear_top_left = center + point(px(-5. * scale), px(-6. * scale));
    path.move_to(rear_top_left);
    path.line_to(center + point(px(3. * scale), px(-6. * scale)));
    path.line_to(center + point(px(3. * scale), px(-3. * scale)));
    path.move_to(center + point(px(-2. * scale), px(3. * scale)));
    path.line_to(center + point(px(-5. * scale), px(3. * scale)));
    path.line_to(rear_top_left);
    rectangle(
        &mut path,
        center + point(px(-2. * scale), px(-3. * scale)),
        px(8. * scale),
        px(9. * scale),
    );

    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn rectangle(path: &mut PathBuilder, top_left: Point<Pixels>, width: Pixels, height: Pixels) {
    path.move_to(top_left);
    path.line_to(top_left + point(width, px(0.)));
    path.line_to(top_left + point(width, height));
    path.line_to(top_left + point(px(0.), height));
    path.close();
}
