use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ops::Range};

/// A template string that will be instantiated with some data.
///
/// The newtype wrapper is used to enforce the distinction between instanitated
/// and uninstantiated templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template(pub String);

impl Template {
    /// Instantiates the template with the given mappings. If a required
    /// mapping is missing, leaves the template placeholder in place.
    ///
    /// This is essentially the same template instantiation algorithm from the
    /// original RepoQuest, but with the data passed in instead of looked up on
    /// the fly.
    pub fn instantiate(&self, data: &HashMap<String, String>) -> Result<String> {
        // TODO: switch to templating engine for non-quadratic behavior.
        let re = Regex::new(r"\{\{ (\S+ \S+) \}\}").unwrap();
        let mut new_body = self.0.clone();
        let substitutions = re.captures_iter(&self.0).filter_map(|cap| {
            let full_match = cap.get(0).unwrap();
            data.get(&cap[1]).map(|sub| (full_match.range(), sub))
        });
        Template::replace_many_ranges(&mut new_body, substitutions);

        Ok(new_body)
    }

    /// Replace ranges with the substitutions.
    ///
    /// This is the same template instantiation algorithm from the original
    /// RepoQuest.
    fn replace_many_ranges(
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
}
