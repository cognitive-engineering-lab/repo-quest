//! # Quest Definition Format
//!
//! This module provies functionality for parsing a dir-based quest format into a
//! [`QuestDefinition`] representation that can be converted into a bundle that can
//! be used by repoquest. An example of the dir-based format follows.
//!
//! ```
//! .
//! ├── .git
//! ├── 00-first
//! │   ├── issue
//! │   │   ├── 00-comment-about-foo.md
//! │   │   └── 01-other-comment.md
//! │   ├── issue.md
//! │   ├── pr
//! │   │   └── 00-comment-with-code-quote.md
//! │   ├── pr.md
//! │   ├── scaffold
//! │   │   ├── 00-prepare-interfaces
//! │   │   │   ├── header
//! │   │   │   ├── other
//! │   │   │   └── README.md
//! │   │   ├── 00-prepare-interfaces.txt
//! │   │   ├── 01-add-placeholders
//! │   │   │   ├── header
//! │   │   │   ├── other
//! │   │   │   ├── README.md
//! │   │   │   └── user
//! │   │   └── 01-add-placeholders.txt
//! │   └── solution
//! │       └── 00-implement-functions
//! │           ├── header
//! │           ├── other
//! │           ├── README.md
//! │           └── user
//! ├── 01-second
//! │   ├── issue.md
//! │   ├── scaffold
//! │   │   └── 00
//! │   │       ├── fixed-header
//! │   │       ├── other
//! │   │       ├── README.md
//! │   │       └── user
//! │   └── solution
//! │       └── 00
//! │           ├── fixed-header
//! │           ├── other
//! │           ├── README.md
//! │           └── user
//! ├── 02-third
//! │   ├── issue.md
//! │   ├── pr.md
//! │   └── solution
//! │       └── 00
//! │           ├── fixed-header
//! │           ├── other
//! │           ├── README.md
//! │           └── user
//! ├── main
//! │   ├── 00
//! │   │   └── README.md
//! │   ├── 00.txt
//! │   ├── 01
//! │   │   ├── other
//! │   │   └── README.md
//! │   └── 01.txt
//! └── quest.txt
//! ```
//!
//! The `quest.md` file containing metadata about the quest definition is
//! required. Otherwise the root quest directory holds only directories, each of
//! which corresponds to a quest chapter. The chapters are ordered
//! lexicographically by directory name. Prefixing the directory names with the
//! chapter numbers is not required, but is our recommended way to ensure the
//! chapters are in the desired order. Directories with names beginning with a
//! `.` (for exmaple, `.git`) are ignored, rather than treated as chapters.
//!
//! `quest.txt` begins with TOML block defining the title, author, repo-name
//! template (for the repository created for the learner), and the compatible
//! RepoQuest version. The body of the file contains a description of the quest
//! which will be displayed to the user after installing the quest. The
//! description is in plain-text, not Markdown.
//!
//! ```txt
//! +++
//! title = "My Test Quest"
//! author = "cognitive-engineering-lab"
//! repo = "my-test-quest"
//! rq-version = "0.3.0"
//! +++
//! Some description of my quest.
//! ```
//!
//! The following entries may appear in each chapter, but only the `issue.md` file
//! and `soulution` directory are required:
//!
//! - `issue.md`: Markdown file containing instructions for the learner. The content
//!   of the file will become the main body of the issue created for the chapter.
//!   The file may start with a frontmatter TOML block defining a title:
//!
//!   ```
//!   +++
//!   title = "My Issue Title"
//!   +++
//!   ```
//!
//!   If it does, that title will be used as the issue title. If not, then the
//!   chapter directory name will be used as the issue title.
//! - `issue/`: Directory containing Markdown files, each of which corresponds to a
//!   comment on the primary issue. The comments are ordered lexicographically by
//!   filename.
//! - `pr.md`: Markdown file containing a description of the scaffolding code (if
//!   any) or additional learner instructions. The content of the file will become
//!   the main body of a pull request for the chapter. If omitted, some default
//!   message linking to the issue will be used as the content of the pull request.
//!
//!   The file may begin with a frontmatter TOML block, just like `issue.md`. If
//!   omitted, the title of the issue will be used as the title of the pull request.
//! - `pr/`: Directory contianing Markdown files, each of which corresponds to a
//!   comment on the pull request. Each comment may begin with a frontmatter TOML
//!   block of the form
//!
//!   ```
//!   +++
//!   file = "path/to/filename.rs"
//!   end-line-side = "right"
//!   end-line = 42
//!   +++
//!   ```
//!
//!   If given, this block defines the code to be quoted for the pull request
//!   comment. `end-line-side` refers to the side of a diff (`"left"` or "`right`"
//!   corresponding to the old and new versions of the file respectively) and
//!   `end-line` is the final line of the quote. Codeberg (which provides the
//!   frontend for RepoQuest) does not support specifying the start line of the
//!   quote and instead uses some heuristic to determine what to include.
//! - `main/`: The initial commits for the quest, before the first chapter.
//!   The format is described below.
//! - `scaffold/`: The scaffolding or set-up for the chapter. This forms the content
//!   of the initial pull request. If omitted, an empty pull request will be
//!   created. The format is described below.
//! - `solution/`: The reference solution for the chapter. This both forms the
//!   content of the reference solution pull request (if requested by the learner)
//!   and the basis from which the diffs to scaffolding for the next chapter are
//!   determined. The format is described below.
//!
//! The `main/`, `scaffold/`, and `solution/` directories represent sequences of
//! commits. Each commit is defined by a directory giving a snapshot of the
//! repository at that point and (optionally) a file (with the same name as the
//! directory but with a `.txt` suffix) whose content is the commit message.
//!
//! # Implementation Notes
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

use serde::Deserialize;

mod parse;
pub use self::parse::parse;
pub use self::parse::parse_quest_commits;

mod bundle;
pub use self::bundle::bundle;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Meta {
    pub title: String,
    pub author: String,
    pub repo: String,
    pub rq_version: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestDefinition {
    pub meta: Meta,
    pub main: Option<Vec<Commit>>,
    pub chapters: Vec<Chapter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestCommits {
    pub main: Option<Vec<Commit>>,
    pub chapters: Vec<ChapterCommits>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterCommits {
    pub branch_name: String,
    /// May be empty.
    pub scaffold: Option<Vec<Commit>>,
    /// Must have at least one commit.
    pub solution: Vec<Commit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub primary_issue: PrimaryIssue,
    pub comments: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimaryIssue {
    pub meta: Option<IssueMeta>,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
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
    pub meta: Option<PullRequestCommentMeta>,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum LineSide {
    Right,
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
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
