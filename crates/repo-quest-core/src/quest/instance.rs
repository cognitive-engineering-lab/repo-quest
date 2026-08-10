//! This module defines the structures that represent instantiated quests.
//!
//! Many of the structures have IDs that refer to parts of the the quest
//! template or to things managed by Forgejo.

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, hash_map::Entry},
    path::{Path, PathBuf},
};

use crate::{fs, git::GitRepo};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub owner: String,
    pub repo: String,
    pub number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub owner: String,
    pub repo: String,
    pub number: i64,
}

/// An instantiated task in a quest.
///
/// The task ID for the defining task template can be determined by other
/// metadata in the quest structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub issue: Issue,
    pub pr: PullRequest,
    /// The git hash of the scaffolding branch at the time when the PR was created.
    pub initial_scaffolding_hash: String,
    pub reference_solution: Option<PullRequest>,
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
    /// The tasks for this quest, indexed by chapter number, in the same order
    /// as the `task_order` field in the `QuestDefinition`. A chapter is `None`
    /// if it has not been instantiated, either because the quest has not
    /// reached it or because it was skipped.
    pub tasks: Vec<Option<Task>>,
    /// The chapter that the quest is currently on, or `None` if no chapter has
    /// been started yet.
    pub current_chapter: Option<usize>,
}

impl QuestMetadata {
    /// Creates a quest with no chapters started, with room for `chapter_count`
    /// chapters.
    #[must_use]
    pub fn new(definition_id: usize, owner: String, repo: String, chapter_count: usize) -> Self {
        QuestMetadata {
            definition_id,
            owner,
            repo,
            tasks: vec![None; chapter_count],
            current_chapter: None,
        }
    }

    /// The total number of chapters in the quest, started or not.
    #[must_use]
    pub fn chapter_count(&self) -> usize {
        self.tasks.len()
    }

    /// The task for the given chapter, if that chapter has been started.
    #[must_use]
    pub fn task(&self, chapter: usize) -> Option<&Task> {
        self.tasks.get(chapter)?.as_ref()
    }

    /// The task for the given chapter, if that chapter has been started.
    pub fn task_mut(&mut self, chapter: usize) -> Option<&mut Task> {
        self.tasks.get_mut(chapter)?.as_mut()
    }

    /// The task for the chapter the quest is currently on.
    #[must_use]
    pub fn current_task(&self) -> Option<&Task> {
        self.task(self.current_chapter?)
    }

    /// The chapter that would follow the current one.
    ///
    /// Note that this is relative to the current chapter, so it does not
    /// account for skipped chapters, which are never returned to.
    #[must_use]
    pub fn next_chapter(&self) -> usize {
        self.current_chapter.map_or(0, |chapter| chapter + 1)
    }

    /// Records `task` as the instantiation of `chapter` and makes that chapter
    /// current.
    pub fn start_chapter(&mut self, chapter: usize, task: Task) -> Result<()> {
        let slot = self
            .tasks
            .get_mut(chapter)
            .with_context(|| format!("Quest has no chapter {chapter}."))?;
        *slot = Some(task);
        self.current_chapter = Some(chapter);
        Ok(())
    }
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
            let file_contents = fs::read_to_string(&index_file, "quest definition index file")?;
            let index = serde_json::from_str(&file_contents).with_context(|| {
                format!(
                    "Could not parse quest definition index from {}",
                    index_file.display()
                )
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
        fs::create_dir_all(&self.dir, "quest instances dir")?;
        let index = serde_json::to_string(&self.index)
            .with_context(|| format!("Could not serialize quest index {:?}", self.index))?;
        let index_file = self.dir.join("data.json");
        fs::write(&index_file, index, "quest index")?;
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[must_use]
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
        let data = fs::read_to_string(&metadata_path, "quest definition file")?;
        serde_json::from_str(&data).with_context(|| {
            format!(
                "Could not parse quest definition from {}",
                metadata_path.display()
            )
        })
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
        fs::create_dir_all(&dir, "quest dir")?;
        let data = serde_json::to_string(quest)
            .with_context(|| format!("Could not serialize quest {quest:?}"))?;
        let quest_file = dir.join("data.json");
        fs::write(&quest_file, data, "quest")?;
        Ok(())
    }

    /// Writes the quest to a file and adds it to the index with the given id.
    ///
    /// Fails if a quest with the given id already exists.
    pub fn insert_quest(&mut self, id: i64, quest: &QuestMetadata) -> Result<()> {
        let dir_name = format!("{}-{id}", quest.definition_id);
        match self.index.entry(id) {
            Entry::Occupied(_) => bail!("Quest with given id {id} already exists."),
            Entry::Vacant(vacant_entry) => vacant_entry.insert(dir_name.into()),
        };
        self.store_quest(id, quest)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quest(chapter_count: usize) -> QuestMetadata {
        QuestMetadata::new(0, "hero".into(), "quest".into(), chapter_count)
    }

    fn task(number: i64) -> Task {
        Task {
            issue: Issue {
                owner: "hero".into(),
                repo: "quest".into(),
                number,
            },
            pr: PullRequest {
                owner: "hero".into(),
                repo: "quest".into(),
                number: number + 1,
            },
            initial_scaffolding_hash: "deadbeef".into(),
            reference_solution: None,
        }
    }

    #[test]
    fn fresh_quest_starts_at_chapter_zero() {
        let quest = quest(3);
        assert_eq!(quest.current_chapter, None);
        assert_eq!(quest.next_chapter(), 0);
        assert!(quest.current_task().is_none());
        assert_eq!(quest.chapter_count(), 3);
    }

    #[test]
    fn starting_a_chapter_makes_it_current() {
        let mut quest = quest(3);
        quest.start_chapter(0, task(1)).unwrap();

        assert_eq!(quest.current_chapter, Some(0));
        assert_eq!(quest.next_chapter(), 1);
        assert_eq!(quest.current_task().unwrap().issue.number, 1);
    }

    #[test]
    fn skipping_leaves_intervening_chapters_uninstantiated() {
        let mut quest = quest(5);
        quest.start_chapter(0, task(1)).unwrap();
        quest.start_chapter(3, task(7)).unwrap();

        assert_eq!(quest.current_chapter, Some(3));
        assert_eq!(quest.next_chapter(), 4);
        assert_eq!(quest.current_task().unwrap().issue.number, 7);
        assert_eq!(quest.task(0).unwrap().issue.number, 1);
        assert!(quest.task(1).is_none(), "chapter 1 was skipped");
        assert!(quest.task(2).is_none(), "chapter 2 was skipped");
        assert_eq!(
            quest.tasks.len(),
            5,
            "skipping does not change the chapter count",
        );
    }

    #[test]
    fn cannot_start_a_chapter_outside_the_quest() {
        let mut quest = quest(2);
        assert!(quest.start_chapter(2, task(1)).is_err());
        assert_eq!(quest.current_chapter, None);
    }

    #[test]
    fn task_mut_reaches_started_chapters_only() {
        let mut quest = quest(2);
        quest.start_chapter(0, task(1)).unwrap();

        quest.task_mut(0).unwrap().reference_solution = Some(PullRequest {
            owner: "hero".into(),
            repo: "quest".into(),
            number: 42,
        });

        assert_eq!(
            quest
                .task(0)
                .unwrap()
                .reference_solution
                .as_ref()
                .unwrap()
                .number,
            42
        );
        assert!(quest.task_mut(1).is_none());
    }
}
