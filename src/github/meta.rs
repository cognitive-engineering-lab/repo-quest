//! This is the metadata given for Quests defined on GitHub.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QuestConfig {
    pub title: String,
    pub author: String,
    pub repo: String,
    pub chapters: Vec<Chapter>,
    pub read_only: Option<Vec<PathBuf>>,
    pub r#final: Option<serde_json::Value>,
    pub final_url: Option<String>,
    pub rq_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Chapter {
    pub label: String,
    pub name: String,
    #[serde(default)]
    pub no_starter: bool,
}
