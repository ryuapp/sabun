use super::scroll::scrollbar_max_offset;
use super::{
    Context, CursorStyle, DiffViewer, FILE_SCROLLBAR_WIDTH, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Palette, ParentElement,
    SCROLLBAR_WIDTH, SOURCE_PICKER_SCROLLBAR_WIDTH, ScrollHandle, ScrollbarAxis, ScrollbarDrag,
    ScrollbarTarget, Styled, canvas, div, fill, px, scrollbar_axis_length, scrollbar_axis_position,
    scrollbar_axis_start, scrollbar_metrics, set_scrollbar_offset, with_alpha,
};
use gpui::DispatchPhase;

impl DiffViewer {
    pub(super) fn render_scrollbar(
        &self,
        target: ScrollbarTarget,
        axis: ScrollbarAxis,
        scroll_handle: ScrollHandle,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let entity = cx.entity();
        let track_color = with_alpha(palette.faint, 0.16);
        let thumb_color = with_alpha(palette.muted, 0.76);
        let id = match target {
            ScrollbarTarget::Files => "files-scrollbar",
            ScrollbarTarget::DiffVertical => "diff-vertical-scrollbar",
            ScrollbarTarget::SourcePicker => "source-picker-scrollbar",
            ScrollbarTarget::WorktreePicker => "worktree-picker-scrollbar",
        };
        let width = match target {
            ScrollbarTarget::Files => FILE_SCROLLBAR_WIDTH,
            ScrollbarTarget::DiffVertical => SCROLLBAR_WIDTH,
            ScrollbarTarget::SourcePicker | ScrollbarTarget::WorktreePicker => {
                SOURCE_PICKER_SCROLLBAR_WIDTH
            }
        };
        let blocks_mouse = scrollbar_max_offset(axis, &scroll_handle) > px(0.);

        let scrollbar = div().id(id).absolute();
        let scrollbar = if blocks_mouse {
            scrollbar.block_mouse_except_scroll()
        } else {
            scrollbar
        };
        let scrollbar = scrollbar.child(
            canvas(
                |_, _, _| (),
                move |track_bounds, (), window, _| {
                    let (thumb_bounds, max_offset, travel) =
                        scrollbar_metrics(axis, track_bounds, &scroll_handle);
                    if max_offset <= px(0.) {
                        return;
                    }

                    window.paint_quad(fill(track_bounds, track_color));
                    window.paint_quad(fill(thumb_bounds, thumb_color));

                    let down_entity = entity.clone();
                    let down_handle = scroll_handle.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                        if phase != DispatchPhase::Capture
                            || event.button != MouseButton::Left
                            || !track_bounds.contains(&event.position)
                            || max_offset <= px(0.)
                        {
                            return;
                        }
                        cx.stop_propagation();

                        let mouse_position = scrollbar_axis_position(axis, event.position);
                        let thumb_start = scrollbar_axis_start(axis, thumb_bounds);
                        let thumb_length = scrollbar_axis_length(axis, thumb_bounds.size);
                        let inside_thumb = if thumb_bounds.contains(&event.position) {
                            mouse_position - thumb_start
                        } else {
                            let inside_thumb = thumb_length / 2.;
                            if travel > px(0.) {
                                let track_start = scrollbar_axis_start(axis, track_bounds);
                                let percentage = ((mouse_position - track_start - inside_thumb)
                                    / travel)
                                    .clamp(0., 1.);
                                set_scrollbar_offset(
                                    &down_handle,
                                    axis,
                                    -(max_offset * percentage),
                                );
                            }
                            inside_thumb
                        };

                        down_entity.update(cx, |this, cx| {
                            this.cancel_smooth_scroll(target);
                            this.scrollbar_drag = Some(ScrollbarDrag {
                                target,
                                inside_thumb,
                            });
                            if target == ScrollbarTarget::SourcePicker {
                                this.maybe_load_more_history(cx);
                            }
                        });
                        cx.notify(down_entity.entity_id());
                    });

                    let up_entity = entity.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase != DispatchPhase::Capture {
                            return;
                        }
                        let is_this_scrollbar = up_entity
                            .read(cx)
                            .scrollbar_drag
                            .is_some_and(|drag| drag.target == target);
                        if is_this_scrollbar {
                            up_entity.update(cx, |this, _| this.scrollbar_drag = None);
                            cx.notify(up_entity.entity_id());
                        }
                    });

                    let move_entity = entity.clone();
                    let move_handle = scroll_handle.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Capture
                            || !event.dragging()
                            || travel <= px(0.)
                            || max_offset <= px(0.)
                        {
                            return;
                        }

                        let Some(drag) = move_entity.read(cx).scrollbar_drag else {
                            return;
                        };
                        if drag.target != target {
                            return;
                        }

                        let track_start = scrollbar_axis_start(axis, track_bounds);
                        let percentage = ((scrollbar_axis_position(axis, event.position)
                            - track_start
                            - drag.inside_thumb)
                            / travel)
                            .clamp(0., 1.);
                        set_scrollbar_offset(&move_handle, axis, -(max_offset * percentage));
                        move_entity.update(cx, |this, cx| {
                            this.cancel_smooth_scroll(target);
                            if target == ScrollbarTarget::SourcePicker {
                                this.maybe_load_more_history(cx);
                            }
                        });
                        cx.notify(move_entity.entity_id());
                    });
                },
            )
            .size_full(),
        );

        let scrollbar = match target {
            ScrollbarTarget::Files => scrollbar,
            ScrollbarTarget::DiffVertical
            | ScrollbarTarget::SourcePicker
            | ScrollbarTarget::WorktreePicker => scrollbar.cursor(CursorStyle::Arrow),
        };

        match axis {
            ScrollbarAxis::Vertical => scrollbar.top_0().right_0().bottom_0().w(px(width)),
        }
        .into_any_element()
    }
}
