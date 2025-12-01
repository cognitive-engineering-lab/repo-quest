//! This module defines the structures for a quest definition.
//!
//! A quest definition consists of a which is kept on-disk and manipulated using
//! the git2 crate, along with templates for tasks. The task templates can refer
//! to themselves and earlier tasks in a way that will be replaced with some
//! kind of link when the templates are instantiated.
//!
//! TODO: Split quest definitions into more files to make them easier to edit by
//! hand.

use anyhow::{Context as _, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    ops::Range,
    path::{Path, PathBuf},
};

use crate::git::GitRepo;

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

/// A normalized git reference name.
///
/// The repository that this reference belongs is determined by where it is
/// used.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRef(pub String);

impl From<String> for GitRef {
    fn from(value: String) -> Self {
        GitRef(value)
    }
}

impl From<&str> for GitRef {
    fn from(value: &str) -> Self {
        GitRef(value.to_string())
    }
}

/// A git commit hash
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitHash(pub String);

/// The lines refered to for a review comment.
///
/// NOTE: Forgejo only supports commenting on a single line of code. By
/// convention we use the last line of the span, since that is the one required
/// by GitHub. The start line is preserved for use when
/// https://codeberg.org/forgejo/forgejo/issues/6093 is implemented.
///
/// TODO: Find a way to make the comment be better preserved when the
/// scaffolding code is merged into the previous student solution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewLineSubject {
    pub start: Option<u64>,
    pub end: u64,
}

/// The thing being commented on for PR review comments.
///
/// Omitting the lines entirely means to refer to the file itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSubject {
    pub commit: GitCommitHash,
    pub file: String,
    pub old_line: Option<ReviewLineSubject>,
    pub new_line: Option<ReviewLineSubject>,
}

/// A comment on a pull request.
///
/// Unlike issue comments, PR comments can refer to some subject from the PR.
///
/// The title is not a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestComment {
    pub quote: Option<ReviewSubject>,
    pub body: Template,
}

/// A template for creating an issue.
///
/// The title is not a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestTemplate {
    pub title: String,
    pub body: Template,
    pub comments: Vec<PullRequestComment>,
}

/// A comment on an issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub body: Template,
}

/// A template for creating an issue.
///
/// The title is not a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTemplate {
    pub title: String,
    pub body: Template,
    pub comments: Vec<Comment>,
}

/// A template for a task in a quest.
///
/// Template placeholders of the form `{ task-id pr }` or `{ task-id issue }`
/// are replaced with appropriate links or shortrefs. Only placeholders
/// referencing this or earlier tasks are replaced (since the issues and PRs for
/// later tasks do not yet exist).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTemplate {
    pub task_id: String,
    /// A template for creating an issue for this task.
    pub issue_template: IssueTemplate,
    /// A template for creating a PR for this task.
    ///
    /// If absent, a standard message referring to the issue is used.
    pub pr_template: Option<PullRequestTemplate>,
    /// Reference to the commit containing the scaffolding code in the defining
    /// repository for the quest containing this task.
    pub scaffolding: GitRef,
    /// Reference to the commit containing the reference solution code in the
    /// defining repository for the quest containing this task.
    pub reference_solution: GitRef,
}

/// A quest definition is made up of a name, a description, a repository, and a
/// collection of task templates.
///
/// TODO: non-linear quests?
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestDefinitionMetadata {
    /// The title of the quest.
    pub title: String,
    // TODO: change to Vec<String> for multiple authors.
    /// The author(s) of the quest.
    pub author: String,
    /// A brief description of the quest.
    pub description: String,
    /// The name to use for the repository generated for the user to use for the
    /// quest.
    ///
    /// The name given here will have a hyphen and number suffixed when it is
    /// required to make the generated name unique. E.g., if a user starts three
    /// quests from the same definition with a `generated_repo_name` of
    /// "quest-name", the first one will be quest-name, the second will be
    /// quest-name-1, and the third will be quest-name-2.
    pub generated_repo_name: String,
    /// The templates defining the tasks for this quest.
    pub tasks: Vec<TaskTemplate>,
    /// A map from the task names used in templates to the internal identifiers
    /// (which are just the chapter numbers).
    pub task_ids: HashMap<String, usize>,
}

/// A collection of quest definitions indexed by an ID.
///
/// On disk the index is a JSON object mapping from quest IDs to the paths for
/// the directories storing the individual quest definitions. The paths are
/// relative to `dir`, which is the directory containing the JSON for the index
/// in `data.json`. The individual quests directories have a `data.json` with
/// the quest metadata, as defined by [`QuestDefinitionMetadata`] and a `git`
/// folder containing the bare git repository defining the quest.
#[derive(Debug, Clone)]
pub struct QuestDefinitionIndex {
    /// Root directory of definitions
    pub dir: PathBuf,
    /// Map from quest ID to the quest definition directory. The directory is
    /// relative to the root directory give by `dir`.
    index: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct QuestDefinition {
    pub metadata: QuestDefinitionMetadata,
    pub repo: GitRepo,
}

impl QuestDefinitionIndex {
    /// Loads the quest definition index, creating it if it does not exist.
    ///
    /// See [`QuestDefinitionIndex`] for the directory format.
    ///
    /// * `param path` - The directory containing the quest definitions
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
            QuestDefinitionIndex { dir, index }
        } else {
            let index = QuestDefinitionIndex { dir, index: vec![] };
            index.store()?;
            index
        };
        Ok(index)
    }

    /// Store the quest definition index. Only stores the index itself, not the
    /// individual quests.
    ///
    /// See [`QuestDefinitionIndex`] for the directory format.
    pub fn store(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("Could not create quest definition dir {:?}.", self.dir))?;
        let index = serde_json::to_string(&self.index).with_context(|| {
            format!("Could not serialize quest definition index {:?}", self.dir)
        })?;
        let index_file = self.dir.join("data.json");
        fs::write(&index_file, index).with_context(|| {
            format!(
                "Could not write quest definition index to file {:?}",
                index_file
            )
        })?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Directory containing the quest definition for the quest with the given
    /// id.
    pub fn dir(&self, id: usize) -> Result<PathBuf> {
        Ok(self.dir.join(
            self.index
                .get(id)
                .with_context(|| format!("No quest definition with id {id}."))?,
        ))
    }

    /// Path to git repository for the quest with the given id.
    pub fn repo_path(&self, id: usize) -> Result<PathBuf> {
        Ok(self.dir(id)?.join("git"))
    }

    /// Git repository for the quest with the given id.
    pub fn repo(&self, id: usize) -> Result<GitRepo> {
        GitRepo::open(self.dir(id)?.join("git"))
    }

    /// Metadata info for the quest with the given id.
    pub fn metadata(&self, id: usize) -> Result<QuestDefinitionMetadata> {
        let path = self.dir(id)?;
        let metadata_path = path.join("data.json");
        let data = fs::read_to_string(&metadata_path)
            .with_context(|| format!("Could not read quest definition file {metadata_path:?}"))?;
        serde_json::from_str(&data)
            .with_context(|| format!("Could not parse quest definition from {metadata_path:?}"))
    }

    /// Quest definition for the quest with the given id.
    pub fn definition(&self, id: usize) -> Result<QuestDefinition> {
        let metadata = self.metadata(id)?;
        let repo = self.repo(id)?;
        Ok(QuestDefinition { metadata, repo })
    }

    /// Add a quest definition and return its id
    pub fn insert(&mut self, path: PathBuf) -> usize {
        self.index.push(path);
        self.index.len() - 1
    }
}
