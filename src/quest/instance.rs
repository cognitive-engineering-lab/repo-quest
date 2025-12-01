//! This module defines the structures that represent instantiated quests.
//!
//! Many of the structures have IDs that refer to parts of the the quest
//! template or to things managed by Forgejo.

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, hash_map::Entry},
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
pub struct QuestMetadata {
    /// The ID of the quest definition that this quest instantiates.
    pub definition_id: usize,
    /// The Forgejo repo owner for this quest.
    pub owner: String,
    /// The Forgejo repo for this quest.
    pub repo: String,
    /// The Forgejo repo URL for this quest
    pub repo_url: Url,
    /// The instantiated tasks for this quest, in the same order as the
    /// `task_order` field in the `QuestDefinition`.
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug)]
pub struct Quest {
    pub dir: PathBuf,
    pub metadata: QuestMetadata,
    pub repo: GitRepo,
}

/// A collection of instantiated quests indexed by ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestInstanceIndex {
    /// Root directory of the instances
    pub dir: PathBuf,
    /// Map from quest instance ID to the quest instance directory. The
    /// directory is relative to the root directory give by `dir`.
    pub index: HashMap<i64, PathBuf>,
}

impl QuestInstanceIndex {
    /// Loads the quest instance index, creating it if it does not exist.
    ///
    /// See [`QuestInstanceIndex`] for the directory format.
    pub fn load_or_init(path: impl AsRef<Path>) -> Result<Self> {
        let dir: PathBuf = path.as_ref().to_path_buf();
        let index_file = dir.join("data.json");
        let index = if index_file.exists() {
            let file_contents = fs::read_to_string(&index_file).with_context(|| {
                format!("Could not read quest definition index file {index_file:?}")
            })?;
            let index = serde_json::from_str(&file_contents).with_context(|| {
                format!("Could not parse quest definition index from {index_file:?}")
            })?;
            QuestInstanceIndex { dir, index }
        } else {
            let index = QuestInstanceIndex {
                dir,
                index: HashMap::new(),
            };
            index.store()?;
            index
        };
        Ok(index)
    }

    /// Store the quest instances index. Only stores the index itself, not the
    /// individual quest instances.
    ///
    /// See [`QuestInstanceIndex`] for the directory format.
    pub fn store(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("Could not create quest instances dir {:?}.", &self.dir))?;
        let index = serde_json::to_string(&self.index)
            .with_context(|| format!("Could not serialize quest index {:?}", self.index))?;
        let index_file = self.dir.join("data.json");
        fs::write(&index_file, index)
            .with_context(|| format!("Could not write quest index to file {:?}", index_file))?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = i64> {
        self.index.keys().copied()
    }

    /// Directory containing the quest instance for the quest with the given
    /// id.
    pub fn dir(&self, id: i64) -> Result<PathBuf> {
        Ok(self.dir.join(
            self.index
                .get(&id)
                .with_context(|| format!("No quest definition with id {id}."))?,
        ))
    }

    /// Path to git repository for the quest with the given id.
    pub fn repo_path(&self, id: i64) -> Result<PathBuf> {
        Ok(self.dir(id)?.join("git"))
    }

    /// Git repository for the quest with the given id.
    pub fn repo(&self, id: i64) -> Result<GitRepo> {
        GitRepo::open(self.dir(id)?.join("git"))
    }

    /// Metadata info for the quest with the given id.
    pub fn metadata(&self, id: i64) -> Result<QuestMetadata> {
        let path = self.dir(id)?;
        let metadata_path = path.join("data.json");
        let data = fs::read_to_string(&metadata_path)
            .with_context(|| format!("Could not read quest definition file {metadata_path:?}"))?;
        serde_json::from_str(&data)
            .with_context(|| format!("Could not parse quest definition from {metadata_path:?}"))
    }

    /// Quest definition for the quest with the given id.
    pub fn quest(&self, id: i64) -> Result<Quest> {
        Ok(Quest {
            dir: self.dir(id)?,
            metadata: self.metadata(id)?,
            repo: self.repo(id)?,
        })
    }

    /// Writes the metadata for a quest to disk.
    pub fn store_quest(&self, id: i64, quest: &QuestMetadata) -> Result<()> {
        let dir = self.dir(id)?;
        fs::create_dir_all(&dir)
            .with_context(|| format!("Could not create quest dir {:?}.", &dir))?;
        let data = serde_json::to_string(quest)
            .with_context(|| format!("Could not serialize quest {:?}", quest))?;
        let quest_file = dir.join("data.json");
        fs::write(&quest_file, data)
            .with_context(|| format!("Could not write quest to file {:?}", quest_file))?;
        Ok(())
    }

    /// Writes the quest to a file and adds it to the index with the given id.
    ///
    /// Fails if a quest with the given id already exists.
    pub fn insert_quest(&mut self, id: i64, quest: &QuestMetadata) -> Result<()> {
        let dir_name = format!("{}-{}", quest.definition_id, &id.to_string());
        match self.index.entry(id) {
            Entry::Occupied(_) => bail!("Quest with given id {id} already exists."),
            Entry::Vacant(vacant_entry) => vacant_entry.insert(dir_name.into()),
        };
        self.store_quest(id, quest)?;
        Ok(())
    }
}
