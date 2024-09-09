use std::ops::Range;

pub fn replace_many_ranges(
  s: &mut String,
  ranges: impl IntoIterator<Item = (Range<usize>, impl AsRef<str>)>,
) {
  let ranges = ranges.into_iter().collect::<Vec<_>>();
  if !ranges.is_empty() {
    debug_assert!((0..ranges.len() - 1).all(|i| ranges[i].0.end <= ranges[i + 1].0.start));
    for (range, content) in ranges.into_iter().rev() {
      s.replace_range(range, content.as_ref());
    }
  }
}
