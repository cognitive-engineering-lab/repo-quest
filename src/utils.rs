use eyre::{Context, Result};
use std::{ops::Range, time::Duration};

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

pub enum RetryError {
  Wait,
  Err(eyre::Error),
}

pub async fn retry_with_timeout<T, Fut>(f: impl Fn() -> Fut) -> eyre::Result<T>
where
  Fut: Future<Output = Result<T, RetryError>>,
{
  const RETRY_INTERVAL: u64 = 500;
  const RETRY_TIMEOUT: u64 = 5000;

  let strategy = tokio_retry::strategy::FixedInterval::from_millis(RETRY_INTERVAL);
  let retry_fut = tokio_retry::RetryIf::spawn(strategy, f, |e: &_| matches!(e, RetryError::Wait));
  let res = tokio::time::timeout(Duration::from_millis(RETRY_TIMEOUT), retry_fut)
    .await
    .context("Operation timed out")?;
  res.map_err(|e| match e {
    RetryError::Err(e) => e,
    RetryError::Wait => unreachable!(),
  })
}

#[test]
fn test_replace_many_ranges() {
  let mut s = "Hello world".to_string();
  replace_many_ranges(&mut s, [(0..1, "Y"), (6..11, "BB")]);
  assert_eq!(s, "Yello BB");
}
