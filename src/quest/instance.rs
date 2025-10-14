//! This module defines the structures that represent instantiated quests.
//!
//! Many of the structures have IDs that refer to parts of the the quest
//! template or to things managed by Forgejo.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use url::Url;

use crate::git::GitRepo;

/// A newtype wrapper for Forgejo issue IDs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueId(pub u64);

/// A newtype wrapper for Forgejo PR IDs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestId(pub u64);

/// An instantiated task in a quest.
///
/// The task ID for the defining task template can be determined by other
/// metadata in the quest structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// issue ID
    pub issue: IssueId,
    /// URL for issue on Forgejo instance
    pub issue_url: Url,
    /// PR ID
    pub pr: PullRequestId,
    /// URL for PR on Forgejo instance
    pub pr_url: Url,
}

/// An instantiated quest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quest {
    /// The ID of the quest definition that this quest instantiates.
    pub definition_id: String,
    /// The Forgejo repo owner for this quest.
    pub owner: String,
    /// The Forgejo repo for this quest.
    pub repo: String,
    /// The Forgejo repo URL for this quest
    pub repo_url: Url,
    /// The local copy of the quest repository
    pub local_repo: GitRepo,
    /// The instantiated tasks for this quest, in the same order as the
    /// `task_order` field in the `QuestDefinition`.
    pub tasks: Vec<Task>,
}

impl Quest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path: &Path = path.as_ref();
        let data = fs::read_to_string(path)
            .with_context(|| format!("Could not read quest file {path:?}"))?;
        let quest = serde_json::from_str(&data)
            .with_context(|| format!("Could not parse quest from {path:?}"))?;
        Ok(quest)
    }

    pub fn store(&self, path: impl AsRef<Path>) -> Result<()> {
        let path: &Path = path.as_ref();
        let quest = serde_json::to_string(self)
            .with_context(|| format!("Could not serialize quest {self:?}"))?;
        fs::write(path, quest)
            .with_context(|| format!("Could not write quest to file {path:?}"))?;
        Ok(())
    }
}

/// A collection of instantiated quests indexed by ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestInstanceIndex {
    /// Map from quest instance ID to the quest data.
    pub quests: HashMap<i64, Quest>,
}

impl QuestInstanceIndex {
    pub fn load_or_init(path: impl AsRef<Path>) -> Result<Self> {
        let path: &Path = path.as_ref();
        let index = if path.exists() {
            let data = fs::read_to_string(path)
                .with_context(|| format!("Could not read quest index file {path:?}"))?;
            serde_json::from_str(&data)
                .with_context(|| format!("Could not parse quest index from {path:?}"))?
        } else {
            let dir = path
                .parent()
                .ok_or(anyhow!("Bad quest instance path {path:?}"))?;
            fs::create_dir_all(dir)
                .with_context(|| format!("Could not create quest instances dir {:?}.", &dir))?;
            let index = QuestInstanceIndex {
                quests: HashMap::new(),
            };
            index.store(path)?;
            index
        };
        Ok(index)
    }

    pub fn store(&self, path: impl AsRef<Path>) -> Result<()> {
        let path: &Path = path.as_ref();
        let quest = serde_json::to_string(self)
            .with_context(|| format!("Could not serialize quest index {self:?}"))?;
        fs::write(path, quest)
            .with_context(|| format!("Could not write quest index to file {path:?}"))?;
        Ok(())
    }
}
