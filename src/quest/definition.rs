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
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use url::Url;

/// A template string that will be instantiated with some data.
///
/// The newtype wrapper is used to enforce the distinction between instanitated
/// and uninstantiated templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template(pub String);

impl Template {
    /// Instantiates the template with the given mappings. Fails if a required
    /// mapping is missing.
    ///
    /// TODO: actually implement--borrow from original repoquest or use the tera crate
    pub fn instantiate(&self, _data: HashMap<String, String>) -> Result<String> {
        Ok(self.0.clone())
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
    /// A template for creating an issue for this task.
    pub issue_template: IssueTemplate,
    /// A template for creating a PR for this task.
    pub pr_template: Option<PullRequestTemplate>,
    /// Reference to the commit containing the scaffolding code in the defining
    /// repository for the quest containing this task.
    pub scaffolding: Option<GitRef>,
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
pub struct QuestDefinition {
    pub name: String,
    pub description: String,
    pub generated_repo_name: String,
    pub repo: PathBuf,
    pub tasks: Vec<TaskTemplate>,
    pub task_ids: HashMap<String, usize>,
}

/// A collection of quest definitions indexed by an ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestDefinitionIndex {
    /// Map from quest ID to the quest definition.
    pub quest_definitions: HashMap<String, QuestDefinition>,
}

fn test_quest(name: &str) -> QuestDefinition {
    QuestDefinition {
        name: name.to_string(),
        generated_repo_name: "my-test-quest".to_string(),
        description: "My Test Quest description. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.".to_string(),
        repo: "my-test-quest".into(),
        tasks: vec![
            TaskTemplate {
                issue_template: IssueTemplate {
                    title: "test issue 1".to_string(),
                    body: Template("test issue body".to_string()),
                    comments: vec![],
                },
                pr_template: PullRequestTemplate {
                    title: "test pr 1".to_string(),
                    body: Template("test pr body".to_string()),
                    comments: vec![],
                },
                scaffolding: GitRef("00-first-scaffolding".to_string()),
                reference_solution: GitRef("00-first-solution".to_string()),
            },
            TaskTemplate {
                issue_template: IssueTemplate {
                    title: "test issue 2".to_string(),
                    body: Template("test issue 2 body".to_string()),
                    comments: vec![],
                },
                pr_template: PullRequestTemplate {
                    title: "test pr 2".to_string(),
                    body: Template("test pr 2 body".to_string()),
                    comments: vec![],
                },
                scaffolding: GitRef("01-next-scaffolding".to_string()),
                reference_solution: GitRef("01-next-solution".to_string()),
            }
        ],
        task_ids: HashMap::from([("00-first".to_string(), 0), ("01-next".to_string(), 1)]),
    }
}

impl QuestDefinitionIndex {
    pub fn load_or_init(path: impl AsRef<Path>) -> Result<Self> {
        let path: &Path = path.as_ref();
        let index = if path.exists() {
            let data = fs::read_to_string(path)
                .with_context(|| format!("Could not read quest definition index file {path:?}"))?;
            serde_json::from_str(&data)
                .with_context(|| format!("Could not parse quest definition index from {path:?}"))?
        } else {
            let dir = path
                .parent()
                .ok_or(anyhow!("Bad quest instance path {path:?}"))?;
            fs::create_dir_all(dir)
                .with_context(|| format!("Could not create quest instances dir {:?}.", &dir))?;
            // for testing
            let mut defs = HashMap::new();
            defs.insert("my-test-quest".to_string(), test_quest("My First Quest"));
            defs.insert(
                "some-other-quest".to_string(),
                test_quest("Some Other Quest"),
            );
            let index = QuestDefinitionIndex {
                quest_definitions: defs,
            };
            // end for testing
            index.store(path)?;
            index
        };
        Ok(index)
    }

    pub fn store(&self, path: impl AsRef<Path>) -> Result<()> {
        let path: &Path = path.as_ref();
        let quest = serde_json::to_string(self)
            .with_context(|| format!("Could not serialize quest definition index {self:?}"))?;
        fs::write(path, quest)
            .with_context(|| format!("Could not write quest definition index to file {path:?}"))?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<&QuestDefinition> {
        self.quest_definitions
            .get(id)
            .ok_or(anyhow!("No quest template id {}.", id))
    }
}
