//! # Quest Definition Format
//!
//! This module provies functionality for parsing a dir-based quest format into a
//! [`QuestDefinition`] representation that can be converted into a bundle that can
//! be used by repoquest.
//!
//! The parsing of the directory structure (mostly) does not default things
//! during parsing, because we want to be able to use this same structure for
//! actions that convert the parsed structure back to the directory structure
//! preserving the user's choices about what to omit (i.e., rather than reifying
//! the defaults).
//!
//! Additionally, we can't normalize things that have the same meaning, because
//! we need to preserve information about what the user wrote. For example, both
//! `None` and an empty `Vec` mean the same thing for the scaffolding directory
//! in terms fo the quest. However, we need to know whether the user created an
//! empty directory or omitted it entirely.
//!
//! This results in, e.g., the [`Chatper`] structure having, a field with type
//! `Option<Vec<_>>`, even though an empty vector has the same meaning as `None`
//! when interpreted as a quest.
use std::path::{Path, PathBuf};

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};

mod parse;
pub use self::parse::parse;

mod bundle;
pub use self::bundle::bundle;

/// A representation of the data in the quest.toml file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct QuestMeta {
    pub title: String,
    pub author: String,
    pub repo: String,
    pub rq_version: String,
    pub description: String,
    /// Must be non-empty
    pub main: Vec<CommitMeta>,
    pub chapters: Vec<ChapterMeta>,
    pub test_cmd: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum TestExpectation {
    Pass,
    Fail,
}

impl TestExpectation {
    /// For defining a default deserailization value
    pub fn pass() -> Self {
        Self::Pass
    }

    pub(crate) fn is_pass(&self) -> bool {
        *self == TestExpectation::Pass
    }
}

/// Representation of commit information written in a quest.toml file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitMeta {
    pub label: String,
    pub expected: TestExpectation,
}

/// Helper structure for serializing and deserializing `CommitMeta`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[serde(untagged)]
pub enum CommitMetaHelper {
    Label(String),
    CommitMeta {
        label: String,
        #[serde(default = "TestExpectation::pass")]
        expected: TestExpectation,
    },
}

impl From<CommitMetaHelper> for CommitMeta {
    fn from(value: CommitMetaHelper) -> Self {
        match value {
            CommitMetaHelper::Label(label) => CommitMeta {
                label,
                expected: TestExpectation::Pass,
            },
            CommitMetaHelper::CommitMeta { label, expected } => CommitMeta { label, expected },
        }
    }
}

impl From<CommitMeta> for CommitMetaHelper {
    fn from(value: CommitMeta) -> Self {
        let CommitMeta { label, expected } = value;
        match expected {
            TestExpectation::Pass => CommitMetaHelper::Label(label),
            TestExpectation::Fail => CommitMetaHelper::CommitMeta { label, expected },
        }
    }
}

impl<'de> Deserialize<'de> for CommitMeta {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(CommitMetaHelper::deserialize(deserializer)?.into())
    }
}

impl Serialize for CommitMeta {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let CommitMeta { label, expected } = self;
        match expected {
            TestExpectation::Pass => serializer.serialize_str(label),
            TestExpectation::Fail => {
                let mut s = serializer.serialize_struct("CommitMeta", 2)?;
                s.serialize_field("label", label)?;
                s.serialize_field("expected", expected)?;
                s.end()
            }
        }
    }
}

impl CommitMeta {
    pub fn into_commit(self, msg: Option<String>, commit_dir: &Path) -> Commit {
        Commit {
            path: commit_dir.join(self.label),
            message: msg,
            expected: self.expected,
        }
    }
}

/// Representation of chapter information written in a quest.toml file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ChapterMeta {
    pub label: String,
    pub scaffold: Option<Vec<CommitMeta>>,
    pub solution: Vec<CommitMeta>,
}

/// The full definition of a quest corresponding to the directory format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestDefinition {
    pub title: String,
    pub author: String,
    pub repo: String,
    pub rq_version: String,
    pub description: String,
    /// Must be non-empty
    pub main: Vec<Commit>,
    pub chapters: Vec<Chapter>,
    /// Absolute path to the assets directory.
    ///
    /// `parse` assumes this will always be "assets" relative to the root of the
    /// quest definition directory.
    pub assets_dir: Option<PathBuf>,
    pub test_cmd: Option<Vec<String>>,
}

impl QuestDefinition {
    pub fn meta(self) -> QuestMeta {
        let chapters = self
            .chapters
            .into_iter()
            .map(|chapter| chapter.meta())
            .collect();
        let main = self
            .main
            .into_iter()
            .map(Commit::into_commit_meta)
            .collect();
        QuestMeta {
            title: self.title,
            author: self.author,
            repo: self.repo,
            rq_version: self.rq_version,
            description: self.description,
            main,
            chapters,
            test_cmd: self.test_cmd,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chapter {
    /// The label identifying the chapter. Must be acceptable as a filename and
    /// as a branch name, since it corresponds to part of each in different
    /// quest representations.
    pub label: String,
    pub issue: Issue,
    pub pull_request: PullRequest,
    /// May be empty.
    pub scaffold: Option<Vec<Commit>>,
    /// Must have at least one commit.
    pub solution: Vec<Commit>,
}

impl Chapter {
    pub fn meta(self) -> ChapterMeta {
        ChapterMeta {
            label: self.label,
            scaffold: self
                .scaffold
                .map(|scaffold| scaffold.into_iter().map(Commit::into_commit_meta).collect()),
            solution: self
                .solution
                .into_iter()
                .map(Commit::into_commit_meta)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueComment {
    /// The absolute path to the defining the issue comment.
    pub path: PathBuf,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub primary_issue: PrimaryIssue,
    pub comments: Option<Vec<IssueComment>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimaryIssue {
    pub meta: Option<IssueMeta>,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct IssueMeta {
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub primary_issue: Option<PrimaryIssue>,
    pub comments: Option<Vec<PullRequestComment>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestComment {
    /// The absolute path to the defining the pull request comment.
    pub path: PathBuf,
    pub meta: Option<PullRequestCommentMeta>,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum LineSide {
    Right,
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct PullRequestCommentMeta {
    pub file: String,
    pub end_line_side: LineSide,
    pub end_line: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    /// The absolute path to the directory with the content of the commit.
    ///
    /// The final directory name must be usable as a branch name, since it will
    /// be used as part of one.
    pub path: PathBuf,
    pub message: Option<String>,
    pub expected: TestExpectation,
}

impl Commit {
    pub fn into_commit_meta(self) -> CommitMeta {
        CommitMeta {
            label: self
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            expected: self.expected,
        }
    }
}

/// This struct represents a kind of a commit, along with what chapter it came
/// from.
///
/// It exists to support iterating over all commits in a quest in a uniform way
/// via `QuestDefinition::commits_iter`.
#[derive(Clone, Copy, Debug)]
pub enum CommitKind<'a> {
    Main,
    Scaffold { chapter_label: &'a str },
    Solution { chapter_label: &'a str },
}

impl<'a> CommitKind<'a> {
    pub fn branch_name(&self, prefix: &str, commit_dir: &Path) -> String {
        let suffix = &commit_dir.file_name().unwrap().to_string_lossy();
        match self {
            CommitKind::Main => format!("{prefix}/main/{suffix}"),
            CommitKind::Scaffold { chapter_label } => {
                format!("{prefix}/chapter/{chapter_label}/scaffold/{suffix}")
            }
            CommitKind::Solution { chapter_label } => {
                format!("{prefix}/chapter/{chapter_label}/solution/{suffix}")
            }
        }
    }
}

impl QuestDefinition {
    /// Iterates commits in order, annotated with the chapter the kind of commit
    /// it is (main, scaffold, solution) along with the chapter it came from.
    pub fn commits_iter(&self) -> impl Iterator<Item = (CommitKind<'_>, &Commit)> {
        let main_iter = self.main.iter().map(|commit| (CommitKind::Main, commit));
        let chapters_iter = self.chapters.iter().flat_map(|chapter| {
            let scaffold_iter = chapter.scaffold.iter().flatten().map(|commit| {
                (
                    CommitKind::Scaffold {
                        chapter_label: &chapter.label,
                    },
                    commit,
                )
            });
            let solution_iter = chapter.solution.iter().map(|commit| {
                (
                    CommitKind::Solution {
                        chapter_label: &chapter.label,
                    },
                    commit,
                )
            });
            scaffold_iter.chain(solution_iter)
        });

        main_iter.chain(chapters_iter)
    }
}
