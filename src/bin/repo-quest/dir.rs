//! # Quest Definition Format
//!
//! This module provies functionality for parsing a dir-based quest format into a
//! [`QuestDefinition`] representation that can be converted into a bundle that can
//! be used by repoquest.
//!
//! The parsing of the directory structure does not default things during
//! parsing, because we want to be able to use this same structure for actions
//! that convert the parsed structure back to the directory structure preserving
//! the user's choices about what to omit (i.e., rather than reifying the
//! defaults).
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
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

mod parse;
pub use self::parse::parse;

mod bundle;
pub use self::bundle::bundle;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct QuestMeta {
    pub title: String,
    pub author: String,
    pub repo: String,
    pub rq_version: String,
    pub description: String,
    pub main: Option<Vec<PathBuf>>,
    pub chapters: Vec<ChapterMeta>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ChapterMeta {
    pub label: String,
    pub scaffold: Option<Vec<PathBuf>>,
    pub solution: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestDefinition {
    pub title: String,
    pub author: String,
    pub repo: String,
    pub rq_version: String,
    pub description: String,
    pub main: Option<Vec<Commit>>,
    pub chapters: Vec<Chapter>,
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
            .map(|main| main.into_iter().map(|commit| commit.path).collect());
        QuestMeta {
            title: self.title,
            author: self.author,
            repo: self.repo,
            rq_version: self.rq_version,
            description: self.description,
            main,
            chapters,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chapter {
    pub branch_name: String,
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
            label: self.branch_name,
            scaffold: self
                .scaffold
                .map(|scaffold| scaffold.into_iter().map(|commit| commit.path).collect()),
            solution: self
                .solution
                .into_iter()
                .map(|commit| commit.path)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueComment {
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
    pub path: PathBuf,
    pub message: Option<String>,
}

// idea
// - create temp dir for storing specific versions of dirs representation
// - build repo directly in target folder
