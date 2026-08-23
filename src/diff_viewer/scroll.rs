use super::{
    Bounds, Context, CursorStyle, DiffViewer, IsZero, MiddleAutoScroll, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, Point, ScrollDelta, ScrollHandle, ScrollWheelEvent,
    ScrollbarAxis, ScrollbarTarget, Size, SmoothScrollTarget, Window, point, px, size,
};
use std::time::{Duration, Instant};

pub(super) const WHEEL_PIXELS_PER_LINE: f32 = 32.;
const WHEEL_SCROLL_SMOOTHING_TIME: Duration = Duration::from_millis(23);
const SCROLL_ANIMATION_MAX_FRAME_TIME: Duration = Duration::from_millis(67);

fn wheel_scroll_smoothing(elapsed: Duration) -> f32 {
    let elapsed = elapsed.min(SCROLL_ANIMATION_MAX_FRAME_TIME).as_secs_f32();
    1. - (-elapsed / WHEEL_SCROLL_SMOOTHING_TIME.as_secs_f32()).exp()
}

pub(super) fn wheel_zoom_direction(delta: ScrollDelta) -> i8 {
    let primary = match delta {
        ScrollDelta::Pixels(delta) => f32::from(if delta.y == px(0.) { delta.x } else { delta.y }),
        ScrollDelta::Lines(delta) => {
            if delta.y == 0. {
                delta.x
            } else {
                delta.y
            }
        }
    };
    if primary > 0. {
        1
    } else if primary < 0. {
        -1
    } else {
        0
    }
}

pub(super) const fn scrollbar_axis_position(
    axis: ScrollbarAxis,
    position: Point<Pixels>,
) -> Pixels {
    match axis {
        ScrollbarAxis::Vertical => position.y,
    }
}

pub(super) const fn scrollbar_axis_length(axis: ScrollbarAxis, size: Size<Pixels>) -> Pixels {
    match axis {
        ScrollbarAxis::Vertical => size.height,
    }
}

pub(super) const fn scrollbar_axis_start(axis: ScrollbarAxis, bounds: Bounds<Pixels>) -> Pixels {
    scrollbar_axis_position(axis, bounds.origin)
}

pub(super) fn scrollbar_max_offset(axis: ScrollbarAxis, scroll_handle: &ScrollHandle) -> Pixels {
    let max_offset = scroll_handle.max_offset();
    match axis {
        ScrollbarAxis::Vertical => max_offset.height,
    }
}

pub(super) fn scrollbar_offset(axis: ScrollbarAxis, scroll_handle: &ScrollHandle) -> Pixels {
    let offset = scroll_handle.offset();
    match axis {
        ScrollbarAxis::Vertical => offset.y,
    }
}

pub(super) fn set_scrollbar_offset(
    scroll_handle: &ScrollHandle,
    axis: ScrollbarAxis,
    value: Pixels,
) {
    let mut offset = scroll_handle.offset();
    match axis {
        ScrollbarAxis::Vertical => offset.y = value,
    }
    scroll_handle.set_offset(offset);
}

pub(super) fn clamp_scroll_offset(
    offset: Point<Pixels>,
    max_offset: Size<Pixels>,
) -> Point<Pixels> {
    point(
        offset.x.clamp(-max_offset.width, px(0.)),
        offset.y.clamp(-max_offset.height, px(0.)),
    )
}

const MIDDLE_AUTO_SCROLL_DEAD_ZONE: f32 = 15.;

pub(super) fn middle_auto_scroll_velocity(displacement: Pixels) -> Pixels {
    // Chromium uses this 2.2-power curve for middle-click autoscroll. Its
    // synthetic fling velocity is expressed per millisecond; sabun integrates
    // pixels per second, hence the 1,000x multiplier here.
    const EXPONENT: f32 = 2.2;
    const MULTIPLIER_PER_SECOND: f32 = 0.008;
    let displacement = f32::from(displacement);
    if displacement.abs() <= MIDDLE_AUTO_SCROLL_DEAD_ZONE {
        return px(0.);
    }
    px(displacement.signum() * displacement.abs().powf(EXPONENT) * MULTIPLIER_PER_SECOND)
}

pub(super) fn vertical_auto_scroll_cursor(displacement: Pixels) -> CursorStyle {
    let displacement = f32::from(displacement);
    if displacement < -MIDDLE_AUTO_SCROLL_DEAD_ZONE {
        CursorStyle::ResizeUp
    } else if displacement > MIDDLE_AUTO_SCROLL_DEAD_ZONE {
        CursorStyle::ResizeDown
    } else {
        CursorStyle::ResizeUpDown
    }
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn windows_vertical_pan_cursor_id(displacement: Pixels) -> u16 {
    let displacement = f32::from(displacement);
    if displacement < -MIDDLE_AUTO_SCROLL_DEAD_ZONE {
        32655
    } else if displacement > MIDDLE_AUTO_SCROLL_DEAD_ZONE {
        32656
    } else {
        32652
    }
}

#[cfg(target_os = "windows")]
pub(super) fn set_native_middle_auto_scroll_cursor(displacement: Pixels) {
    set_windows_cursor_resource(windows_vertical_pan_cursor_id(displacement));
}

#[cfg(not(target_os = "windows"))]
pub(super) const fn set_native_middle_auto_scroll_cursor(_displacement: Pixels) {}

#[cfg(target_os = "windows")]
pub(super) fn restore_native_cursor() {
    set_windows_cursor_resource(32512);
}

#[cfg(not(target_os = "windows"))]
pub(super) const fn restore_native_cursor() {}

#[cfg(target_os = "windows")]
pub(super) fn set_windows_cursor_resource(resource_id: u16) {
    use std::ffi::c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn LoadCursorW(instance: *mut c_void, cursor_name: *const u16) -> *mut c_void;
        fn SetCursor(cursor: *mut c_void) -> *mut c_void;
    }

    // These are shared system resources returned by User32 and must not be destroyed by sabun.
    unsafe {
        let cursor = LoadCursorW(std::ptr::null_mut(), resource_id as usize as *const u16);
        if !cursor.is_null() {
            SetCursor(cursor);
        }
    }
}

pub(super) fn accumulate_scroll_target(
    current: Point<Pixels>,
    queued: Point<Pixels>,
    delta: Point<Pixels>,
    max_offset: Size<Pixels>,
) -> Point<Pixels> {
    fn axis(current: Pixels, queued: Pixels, delta: Pixels, max_offset: Pixels) -> Pixels {
        let pending = queued - current;
        let reverses_direction =
            (delta > px(0.) && pending < px(0.)) || (delta < px(0.) && pending > px(0.));
        let base = if reverses_direction { current } else { queued };
        (base + delta).clamp(-max_offset, px(0.))
    }

    point(
        axis(current.x, queued.x, delta.x, max_offset.width),
        axis(current.y, queued.y, delta.y, max_offset.height),
    )
}

pub(super) fn scrollbar_metrics(
    axis: ScrollbarAxis,
    track_bounds: Bounds<Pixels>,
    scroll_handle: &ScrollHandle,
) -> (Bounds<Pixels>, Pixels, Pixels) {
    let track_length = scrollbar_axis_length(axis, track_bounds.size).max(px(0.));
    let viewport_length = scrollbar_axis_length(axis, scroll_handle.bounds().size).max(px(1.));
    let max_offset = scrollbar_max_offset(axis, scroll_handle);
    let thumb_length = if max_offset > px(0.) {
        (track_length * (viewport_length / (viewport_length + max_offset)))
            .clamp(px(28.), track_length)
    } else {
        track_length
    };
    let travel = (track_length - thumb_length).max(px(0.));
    let current_offset = (-scrollbar_offset(axis, scroll_handle)).clamp(px(0.), max_offset);
    let thumb_offset = if max_offset > px(0.) {
        travel * (current_offset / max_offset)
    } else {
        px(0.)
    };

    let thumb_bounds = match axis {
        ScrollbarAxis::Vertical => Bounds::new(
            point(track_bounds.left(), track_bounds.top() + thumb_offset),
            size(track_bounds.size.width, thumb_length),
        ),
    };

    (thumb_bounds, max_offset, travel)
}

impl DiffViewer {
    fn scroll_handle_for(&self, target: SmoothScrollTarget) -> ScrollHandle {
        match target {
            SmoothScrollTarget::Files => self.file_scroll.clone(),
            SmoothScrollTarget::Diff => self.diff_scroll.clone(),
        }
    }

    fn smooth_scroll_parts(
        &mut self,
        target: SmoothScrollTarget,
    ) -> (ScrollHandle, &mut super::SmoothScrollState) {
        match target {
            SmoothScrollTarget::Files => (self.file_scroll.clone(), &mut self.file_smooth_scroll),
            SmoothScrollTarget::Diff => (self.diff_scroll.clone(), &mut self.diff_smooth_scroll),
        }
    }

    pub(super) fn scroll_wheel(
        &mut self,
        target: SmoothScrollTarget,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if target == SmoothScrollTarget::Diff && event.modifiers.control {
            self.zoom_diff(event, window, cx);
            cx.stop_propagation();
            return;
        }
        self.text_context_menu = None;
        self.path_context_menu = None;
        let raw_delta = event.delta.pixel_delta(px(WHEEL_PIXELS_PER_LINE));
        let delta = match target {
            SmoothScrollTarget::Files => point(
                px(0.),
                if raw_delta.y.is_zero() {
                    raw_delta.x
                } else {
                    raw_delta.y
                },
            ),
            SmoothScrollTarget::Diff => {
                if !raw_delta.x.is_zero() && !raw_delta.y.is_zero() {
                    if raw_delta.x.abs() > raw_delta.y.abs() {
                        point(raw_delta.x, px(0.))
                    } else {
                        point(px(0.), raw_delta.y)
                    }
                } else {
                    raw_delta
                }
            }
        };
        if delta.x.is_zero() && delta.y.is_zero() {
            return;
        }

        let (scroll_handle, state) = self.smooth_scroll_parts(target);
        let current = scroll_handle.offset();
        let max_offset = scroll_handle.max_offset();

        if matches!(event.delta, ScrollDelta::Pixels(_)) {
            let next = clamp_scroll_offset(current + delta, max_offset);
            state.stop_at(next);
            if next != current {
                scroll_handle.set_offset(next);
                cx.notify();
            }
            return;
        }

        let previous_target = if state.running { state.target } else { current };
        state.target = accumulate_scroll_target(current, previous_target, delta, max_offset);
        if state.target == previous_target {
            return;
        }

        if !state.running {
            state.running = true;
            state.last_frame = Some(Instant::now());
            cx.on_next_frame(window, move |this, window, cx| {
                this.animate_scroll(target, window, cx);
            });
        }
    }

    pub(super) fn toggle_middle_auto_scroll(
        &mut self,
        target: SmoothScrollTarget,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.cancel_middle_auto_scroll(cx) {
            cx.stop_propagation();
            return;
        }
        if self.scroll_handle_for(target).max_offset().height <= px(0.) {
            return;
        }
        self.cancel_smooth_scroll(target.scrollbar_target());
        self.middle_auto_scroll = Some(MiddleAutoScroll {
            target,
            anchor: event.position,
            cursor: event.position,
            last_frame: Instant::now(),
        });
        set_native_middle_auto_scroll_cursor(px(0.));
        cx.on_next_frame(window, |this, window, cx| {
            this.animate_middle_auto_scroll(window, cx);
        });
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn update_middle_auto_scroll_cursor(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.middle_auto_scroll.as_mut() else {
            return;
        };
        state.cursor = event.position;
        set_native_middle_auto_scroll_cursor(state.cursor.y - state.anchor.y);
        cx.stop_propagation();
    }

    fn animate_middle_auto_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        let Some(state) = self.middle_auto_scroll.as_mut() else {
            return;
        };
        let elapsed = now
            .saturating_duration_since(state.last_frame)
            .min(SCROLL_ANIMATION_MAX_FRAME_TIME);
        state.last_frame = now;
        let state = *state;
        let scroll_handle = self.scroll_handle_for(state.target);
        let displacement = state.cursor - state.anchor;
        let velocity = point(
            middle_auto_scroll_velocity(displacement.x),
            middle_auto_scroll_velocity(displacement.y),
        );
        let velocity = match state.target {
            SmoothScrollTarget::Files => point(px(0.), velocity.y),
            SmoothScrollTarget::Diff => velocity,
        };
        let current = scroll_handle.offset();
        let movement = velocity * elapsed.as_secs_f32();
        let next = clamp_scroll_offset(current - movement, scroll_handle.max_offset());
        if next != current {
            scroll_handle.set_offset(next);
            cx.notify();
        }
        cx.on_next_frame(window, |this, window, cx| {
            this.animate_middle_auto_scroll(window, cx);
        });
    }

    pub(super) fn cancel_middle_auto_scroll_on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Middle {
            self.cancel_middle_auto_scroll(cx);
        }
    }

    pub(super) fn cancel_middle_auto_scroll(&mut self, cx: &mut Context<Self>) -> bool {
        if self.middle_auto_scroll.take().is_none() {
            return false;
        }
        restore_native_cursor();
        cx.notify();
        true
    }

    fn animate_scroll(
        &mut self,
        target: SmoothScrollTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (scroll_handle, state) = self.smooth_scroll_parts(target);
        let current = scroll_handle.offset();
        let max_offset = scroll_handle.max_offset();
        if !state.running {
            return;
        }

        let now = Instant::now();
        let elapsed = state
            .last_frame
            .replace(now)
            .map_or(Duration::ZERO, |last_frame| {
                now.saturating_duration_since(last_frame)
            });
        state.target = clamp_scroll_offset(state.target, max_offset);
        let distance = state.target - current;
        let settled = distance.x.abs() <= px(0.5) && distance.y.abs() <= px(0.5);
        let next = if settled {
            let target = state.target;
            state.stop_at(target);
            target
        } else {
            let smoothing = wheel_scroll_smoothing(elapsed);
            point(
                current.x + distance.x * smoothing,
                current.y + distance.y * smoothing,
            )
        };
        scroll_handle.set_offset(next);
        cx.notify();

        if state.running {
            cx.on_next_frame(window, move |this, window, cx| {
                this.animate_scroll(target, window, cx);
            });
        }
    }

    pub(super) fn cancel_smooth_scroll(&mut self, target: ScrollbarTarget) {
        let Some(smooth_target) = target.smooth_target() else {
            return;
        };
        let (scroll_handle, state) = self.smooth_scroll_parts(smooth_target);
        state.stop_at(scroll_handle.offset());
    }
}
