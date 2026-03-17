use anyhow::Result;
use mustache::Data;
use serde::{Deserialize, Serialize};

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
    pub fn instantiate(&self, data: &Data) -> Result<String> {
        let template = mustache::compile_str(&self.0).unwrap();

        Ok(template.render_data_to_string(data)?)
    }
}
