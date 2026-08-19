use super::{Pixels, px};

pub(super) fn cumulative_offsets<T>(
    rows: &[T],
    mut row_height: impl FnMut(&T) -> Pixels,
) -> Vec<Pixels> {
    let mut offsets = Vec::with_capacity(rows.len() + 1);
    let mut total = px(0.);
    offsets.push(total);
    for row in rows {
        total += row_height(row);
        offsets.push(total);
    }
    offsets
}

pub(super) fn row_index_at_position(
    offsets: &[Pixels],
    position: Pixels,
    row_count: usize,
) -> usize {
    offsets
        .partition_point(|top| *top <= position)
        .saturating_sub(1)
        .min(row_count.saturating_sub(1))
}
