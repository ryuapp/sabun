use super::{Algorithm, ChangeTag, Range, diff_graphemes, diff_unicode_words};

pub(super) fn inline_ranges(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    const MERGE_GAP: usize = 4;
    const MIN_SHARED_PERCENT: usize = 30;

    let mut old_ranges = Vec::new();
    let mut new_ranges = Vec::new();
    let mut old_offset = 0;
    let mut new_offset = 0;
    let mut shared_significant_chars = 0;

    for (tag, segment) in diff_unicode_words(Algorithm::Myers, old, new) {
        match tag {
            ChangeTag::Equal => {
                shared_significant_chars +=
                    segment.chars().filter(|ch| !ch.is_whitespace()).count();
                old_offset += segment.len();
                new_offset += segment.len();
            }
            ChangeTag::Delete => {
                old_ranges.push(old_offset..old_offset + segment.len());
                old_offset += segment.len();
            }
            ChangeTag::Insert => {
                new_ranges.push(new_offset..new_offset + segment.len());
                new_offset += segment.len();
            }
        }
    }

    let largest_significant_len = old
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .count()
        .max(new.chars().filter(|ch| !ch.is_whitespace()).count());
    let identifier_replacement = single_identifier_replacement(old, new, &old_ranges, &new_ranges);
    if !identifier_replacement
        && (largest_significant_len == 0
            || shared_significant_chars.saturating_mul(100)
                < largest_significant_len.saturating_mul(MIN_SHARED_PERCENT))
    {
        return (Vec::new(), Vec::new());
    }

    if identifier_replacement {
        return grapheme_ranges(old, new);
    }

    (
        merge_nearby_ranges(old_ranges, MERGE_GAP),
        merge_nearby_ranges(new_ranges, MERGE_GAP),
    )
}

fn single_identifier_replacement(
    old: &str,
    new: &str,
    old_ranges: &[Range<usize>],
    new_ranges: &[Range<usize>],
) -> bool {
    let ([old_range], [new_range]) = (old_ranges, new_ranges) else {
        return false;
    };
    [&old[old_range.clone()], &new[new_range.clone()]]
        .into_iter()
        .all(|text| {
            !text.is_empty()
                && text
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric())
        })
}

fn grapheme_ranges(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let mut old_ranges = Vec::new();
    let mut new_ranges = Vec::new();
    let mut old_offset = 0;
    let mut new_offset = 0;
    for (tag, segment) in diff_graphemes(Algorithm::Myers, old, new) {
        match tag {
            ChangeTag::Equal => {
                old_offset += segment.len();
                new_offset += segment.len();
            }
            ChangeTag::Delete => {
                old_ranges.push(old_offset..old_offset + segment.len());
                old_offset += segment.len();
            }
            ChangeTag::Insert => {
                new_ranges.push(new_offset..new_offset + segment.len());
                new_offset += segment.len();
            }
        }
    }
    (old_ranges, new_ranges)
}

fn merge_nearby_ranges(ranges: Vec<Range<usize>>, maximum_gap: usize) -> Vec<Range<usize>> {
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start.saturating_sub(previous.end) <= maximum_gap
        {
            previous.end = previous.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    merged
}
