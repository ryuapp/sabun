use std::time::{Duration, Instant};

use super::layout::wrapped_row_offsets;
use super::scroll::wheel_zoom_direction;
use super::{
    Context, DEFAULT_DIFF_FONT_SIZE, DEFAULT_DIFF_GUTTER_FONT_SIZE, DIFF_CODE_LINE_HEIGHT,
    DIFF_CODE_PADDING_Y, DiffViewer, Palette, Pixels, ScrollWheelEvent, Window, point, px,
    wrapped_code_width,
};

const MIN_DIFF_FONT_SIZE: f32 = 8.;
const MAX_DIFF_FONT_SIZE: f32 = 24.;
const DIFF_FONT_ZOOM_FACTOR: f32 = 1.1;
const DIFF_FONT_ZOOM_EPSILON: f32 = 0.02;
const DIFF_FONT_ZOOM_SMOOTHING_TIME: Duration = Duration::from_millis(24);
const DIFF_FONT_ZOOM_MAX_SPEED: f32 = 60.;
const DIFF_FONT_ZOOM_MAX_FRAME_TIME: Duration = Duration::from_millis(67);
const DIFF_FONT_ZOOM_INPUT_EASING: f32 = 0.52;
const DIFF_FONT_ZOOM_INPUT_MAX_STEP: f32 = 0.75;

fn zoom_frame_parameters(elapsed: Duration) -> (f32, f32) {
    let elapsed_seconds = elapsed.min(DIFF_FONT_ZOOM_MAX_FRAME_TIME).as_secs_f32();
    let smoothing_seconds = DIFF_FONT_ZOOM_SMOOTHING_TIME.as_secs_f32();
    let easing = 1. - (-elapsed_seconds / smoothing_seconds).exp();
    let maximum_step = DIFF_FONT_ZOOM_MAX_SPEED * elapsed_seconds;
    (easing, maximum_step)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DiffZoomAnchor {
    display_index: usize,
    row_fraction: f32,
}

impl DiffViewer {
    pub(super) fn diff_code_line_height_for(font_size: Pixels) -> Pixels {
        px(DIFF_CODE_LINE_HEIGHT) * (f32::from(font_size) / DEFAULT_DIFF_FONT_SIZE)
    }

    pub(super) fn diff_code_padding_y_for(font_size: Pixels) -> Pixels {
        px(DIFF_CODE_PADDING_Y) * (f32::from(font_size) / DEFAULT_DIFF_FONT_SIZE)
    }

    pub(super) fn diff_code_line_height(&self) -> Pixels {
        Self::diff_code_line_height_for(self.diff_font_size)
    }

    pub(super) fn diff_code_row_height(&self) -> Pixels {
        self.diff_code_line_height() + self.diff_code_padding_y() * 2.
    }

    pub(super) fn diff_code_padding_y(&self) -> Pixels {
        Self::diff_code_padding_y_for(self.diff_font_size)
    }

    pub(super) fn diff_gutter_font_size(&self) -> Pixels {
        self.diff_font_size * (DEFAULT_DIFF_GUTTER_FONT_SIZE / DEFAULT_DIFF_FONT_SIZE)
    }

    pub(super) fn calculate_wrapped_row_offsets(
        &self,
        code_width: Pixels,
        font_size: Pixels,
        window: &Window,
        palette: Palette,
    ) -> (Vec<Pixels>, super::WrapWidthRange) {
        wrapped_row_offsets(
            &self.diff_row_layouts,
            code_width,
            font_size,
            Self::diff_code_line_height_for(font_size),
            Self::diff_code_padding_y_for(font_size),
            window,
            palette,
        )
    }

    pub(super) fn current_diff_zoom_anchor(&self) -> Option<DiffZoomAnchor> {
        let visible_top = (-self.diff_scroll.offset().y).max(px(0.));
        let offsets = self.diff_offsets();
        let display_index = offsets.row_index_at(visible_top, self.diff_rows.len());
        let row_top = offsets.get(display_index)?;
        let row_bottom = offsets.get(display_index + 1)?;
        let row_height = (row_bottom - row_top).max(px(1.));
        Some(DiffZoomAnchor {
            display_index,
            row_fraction: ((visible_top - row_top) / row_height).clamp(0., 1.),
        })
    }

    pub(super) fn restore_diff_zoom_anchor(&mut self, anchor: DiffZoomAnchor) {
        let offsets = self.diff_offsets();
        let Some(row_top) = offsets.get(anchor.display_index) else {
            return;
        };
        let Some(row_bottom) = offsets.get(anchor.display_index + 1) else {
            return;
        };
        let visible_top = row_top + (row_bottom - row_top) * anchor.row_fraction;
        let content_height = offsets.last().unwrap_or_default();
        let viewport_height = self.diff_scroll.bounds().size.height;
        let max_y = (content_height - viewport_height).max(px(0.));
        let current = self.diff_scroll.offset();
        let target = point(current.x, (-visible_top).clamp(-max_y, px(0.)));
        self.diff_scroll.set_offset(target);
        self.diff_smooth_scroll.stop_at(target);
    }

    fn start_diff_layout_zoom(&mut self, font_size: Pixels, window: &mut Window) {
        let code_width = wrapped_code_width(
            window.viewport_size().width,
            self.sidebar_width,
            self.layout,
            font_size,
        )
        .floor();
        let (target, width_range) =
            self.calculate_wrapped_row_offsets(code_width, font_size, window, self.palette());
        self.diff_layout_font_size = font_size;
        self.wrapped_offsets_range = Some(width_range);
        self.pending_scroll_file = None;

        if self.diff_row_offsets_target.is_none() {
            self.diff_layout_zoom_anchor = self.current_diff_zoom_anchor();
            self.diff_layout_zoom_start_font_size = self.diff_font_size;
        }
        self.diff_row_offsets_target = Some(target);
        self.update_diff_layout_zoom_progress();
    }

    pub(super) fn cancel_diff_layout_zoom(&mut self) {
        if let Some(target) = self.diff_row_offsets_target.take() {
            self.diff_row_offsets = target;
        }
        self.diff_row_offsets_progress = 0.;
        self.diff_layout_zoom_anchor = None;
    }

    fn update_diff_layout_zoom_progress(&mut self) {
        if self.diff_row_offsets_target.is_none() {
            return;
        }
        let start = f32::from(self.diff_layout_zoom_start_font_size);
        let target = f32::from(self.diff_font_animation_target);
        let distance = target - start;
        self.diff_row_offsets_progress = if distance.abs() <= f32::EPSILON {
            1.
        } else {
            ((f32::from(self.diff_font_size) - start) / distance).clamp(0., 1.)
        };

        if let Some(anchor) = self.diff_layout_zoom_anchor {
            self.restore_diff_zoom_anchor(anchor);
        }
    }

    fn finish_diff_layout_zoom(&mut self) {
        if let Some(target) = self.diff_row_offsets_target.take() {
            self.diff_row_offsets = target;
        }
        self.diff_row_offsets_progress = 0.;
        self.diff_layout_zoom_start_font_size = self.diff_font_size;
        self.diff_layout_zoom_anchor = None;
    }

    fn advance_diff_zoom(&mut self, easing: f32, maximum_step: f32) {
        let current = f32::from(self.diff_font_size);
        let target = f32::from(self.diff_font_animation_target);
        let distance = target - current;
        if distance.abs() <= DIFF_FONT_ZOOM_EPSILON {
            self.diff_font_size = self.diff_font_animation_target;
            self.update_diff_layout_zoom_progress();
            self.finish_diff_layout_zoom();
            if self.diff_font_animation_target == self.diff_font_size_target {
                self.diff_font_zoom_running = false;
                self.diff_font_zoom_last_frame = None;
            }
        } else {
            let step = (distance * easing).clamp(-maximum_step, maximum_step);
            self.diff_font_size = px(current + step);
            self.update_diff_layout_zoom_progress();
        }
    }

    pub(super) fn zoom_diff(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let direction = wheel_zoom_direction(event.delta);
        if direction == 0 {
            return;
        }
        let current_target = f32::from(self.diff_font_size_target);
        let next_target = if direction > 0 {
            current_target * DIFF_FONT_ZOOM_FACTOR
        } else {
            current_target / DIFF_FONT_ZOOM_FACTOR
        }
        .clamp(MIN_DIFF_FONT_SIZE, MAX_DIFF_FONT_SIZE);
        let next_target = px(next_target);
        if next_target == self.diff_font_size_target {
            return;
        }

        self.diff_font_size_target = next_target;
        self.text_context_menu = None;
        self.path_context_menu = None;
        self.diff_smooth_scroll.stop_at(self.diff_scroll.offset());

        let animation_was_running = self.diff_font_zoom_running;
        self.diff_font_zoom_running = true;
        if !animation_was_running {
            self.diff_font_zoom_last_frame = Some(Instant::now());
        }
        self.diff_font_animation_target = next_target;
        self.start_diff_layout_zoom(next_target, window);
        self.advance_diff_zoom(DIFF_FONT_ZOOM_INPUT_EASING, DIFF_FONT_ZOOM_INPUT_MAX_STEP);
        cx.notify();

        if !animation_was_running && self.diff_font_zoom_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.animate_diff_zoom(window, cx);
            });
        }
    }

    fn animate_diff_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.diff_font_zoom_running {
            return;
        }

        let now = Instant::now();
        let elapsed = self
            .diff_font_zoom_last_frame
            .replace(now)
            .map_or(Duration::ZERO, |previous| {
                now.saturating_duration_since(previous)
            });
        let (easing, maximum_step) = zoom_frame_parameters(elapsed);
        self.advance_diff_zoom(easing, maximum_step);
        cx.notify();

        if self.diff_font_zoom_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.animate_diff_zoom(window, cx);
            });
        }
    }
}
