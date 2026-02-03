/*!
Example file layout:

```
.
├── 00-first
│   ├── issue
│   │   ├── 00-comment-about-foo.md
│   │   └── 01-other-comment.md
│   ├── issue.md
│   ├── pr
│   │   └── 00-comment-with-code-quote.md
│   ├── pr.md
│   ├── scaffold
│   │   ├── 00-prepare-interfaces
│   │   │   └── header
│   │   ├── 00-prepare-interfaces.txt
│   │   ├── 01-add-placeholders
│   │   │   ├── header
│   │   │   └── user
│   │   └── 01-add-placeholders.txt
│   └── solution
│       └── 00-implement-functions
│           ├── header
│           └── user
├── 01-second
│   ├── issue.md
│   ├── scaffold
│   │   └── 00
│   │       ├── fixed-header
│   │       └── user
│   └── solution
│       └── 00
│           ├── fixed-header
│           └── user
├── 02-third
│   ├── issue.md
│   ├── pr.md
│   └── solution
│       └── 00
│           ├── fixed-header
│           └── user
└── meta.toml
```

Note that the parsing of the structure cannot default things during parsing,
because we want to be able to use this same structure for conversions back and
forth while preserving the user's choices about what to omit (rather than
reifying the defaults).

Additionall, we can't normalize things that have the same meaning, because we
need to preserve information about what the user wrote. For example, both `None`
and an empty `Vec` mean the same thing for the scaffolding directory in terms fo
the quest. However, we need to know whether the user created an empty directory
or omitted it entirely.
*/
use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use log::{debug, warn};
use regex::Regex;
use serde::Deserialize;

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Meta {
    pub title: String,
    pub author: String,
    pub repo: String,
    pub rq_version: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct QuestDefinition {
    meta: Meta,
    chapters: Vec<Chapter>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Chapter {
    pub branch_name: String,
    pub issue: Issue,
    pub pull_request: PullRequest,
    /// May be empty.
    pub scaffold: Option<Vec<Commit>>,
    /// Must have at least one commit.
    pub solution: Vec<Commit>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Issue {
    pub primary_issue: PrimaryIssue,
    pub comments: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrimaryIssue {
    pub meta: Option<IssueMeta>,
    pub content: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct IssueMeta {
    pub title: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub primary_issue: Option<PrimaryIssue>,
    pub comments: Vec<PullRequestComment>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PullRequestComment {
    pub meta: Option<PullRequestCommentMeta>,
    pub content: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum LineSide {
    Right,
    Left,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct PullRequestCommentMeta {
    pub end_line_side: LineSide,
    pub end_line: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Commit {
    pub path: PathBuf,
    pub message: Option<String>,
}

pub fn bundle_dir(output: PathBuf, input: PathBuf) -> Result<()> {
    Ok(())
}

pub fn parse(dir: &Path) -> Result<QuestDefinition> {
    let meta: Meta = toml::from_str(&fs::read_to_string(dir.join("quest.toml"))?)?;
    let chapters = parse_chapters(dir)?;
    Ok(QuestDefinition { meta, chapters })
}

fn parse_chapters(dir: &Path) -> Result<Vec<Chapter>> {
    let chapter_dirs: Vec<PathBuf> = read_dir_sorted_paths(dir)?
        .into_iter()
        .filter(|path| path.is_dir())
        .collect();
    debug!("Chapters dirs: {:?}", chapter_dirs);

    let mut chapters = Vec::with_capacity(chapter_dirs.len());
    for chapter_dir in chapter_dirs {
        chapters.push(parse_chapter(chapter_dir)?);
    }

    Ok(chapters)
}

fn parse_chapter(chapter_dir: PathBuf) -> Result<Chapter> {
    debug!("Parsing chapters dir: {:?}", chapter_dir);

    let branch_name = chapter_dir
        .file_name()
        .with_context(|| format!("Could not extract branchanme from {:?}.", chapter_dir))?
        .to_string_lossy()
        .into_owned();
    let issue = parse_issue(&chapter_dir)?;
    let pull_request = parse_pull_request(&chapter_dir)?;
    println!("{:?}", chapter_dir);
    let scaffold_dir = chapter_dir.join("scaffold");
    let scaffold = if scaffold_dir.exists() {
        Some(parse_commits_dir(&scaffold_dir)?)
    } else {
        None
    };
    let solution = parse_commits_dir(&chapter_dir.join("solution"))?;

    Ok(Chapter {
        branch_name,
        issue,
        pull_request,
        scaffold,
        solution,
    })
}

fn parse_issue(chapter_dir: &Path) -> Result<Issue> {
    let primary_issue = parse_primary_issue(File::open(chapter_dir.join("issue.md"))?)?;
    let comments = parse_issue_comments(&chapter_dir.join("issue"))?;

    Ok(Issue {
        primary_issue,
        comments,
    })
}

fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let delimiter = Regex::new(r"(?m)^\+\+\+\n").unwrap();

    if let Some((frontmatter, content)) =
        content
            .trim_start()
            .strip_prefix("+++\n")
            .and_then(|content| {
                let splits = delimiter.splitn(content, 2);
                if let &[frontmatter, content] = splits.collect::<Vec<&str>>().as_slice() {
                    Some((frontmatter, content))
                } else {
                    // no end delimiter
                    None
                }
            })
    {
        (Some(frontmatter), content)
    } else {
        (None, content)
    }
}

fn parse_primary_issue(issue_data: impl Read) -> Result<PrimaryIssue> {
    let issue_file_content = io::read_to_string(issue_data)?;

    let (frontmatter, content) = split_frontmatter(&issue_file_content);

    if let Some(frontmatter) = frontmatter {
        Ok(PrimaryIssue {
            meta: Some(toml::from_str(frontmatter)?),
            content: content.to_string(),
        })
    } else {
        Ok(PrimaryIssue {
            meta: None,
            content: issue_file_content,
        })
    }
}

fn read_dir_sorted_paths(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = read_dir_paths(dir)?;
    paths.sort();
    Ok(paths)
}

fn read_dir_paths(dir: &Path) -> io::Result<Vec<PathBuf>> {
    Ok(dir
        .read_dir()?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .map(|entry| entry.path())
        .collect())
}

fn parse_issue_comments(comments_dir: &Path) -> Result<Vec<String>> {
    let mut comments = Vec::new();
    if comments_dir.exists() {
        for path in read_dir_sorted_paths(comments_dir)? {
            if path.is_file() && path.extension().is_some_and(|extension| extension == "md") {
                comments.push(fs::read_to_string(path)?);
            } else {
                warn!(
                    "Issue comments directory {:?} contains non-.md file {:?}",
                    comments_dir, path
                );
            }
        }
    }
    Ok(comments)
}

fn parse_pull_request(chapter_dir: &Path) -> Result<PullRequest> {
    let pr_path = chapter_dir.join("pr.md");
    // PR primary issue is optional.
    let primary_issue = if pr_path.exists() {
        Some(parse_primary_issue(File::open(pr_path)?)?)
    } else {
        None
    };

    let comments = parse_pull_request_comments(&chapter_dir.join("pr"))?;

    Ok(PullRequest {
        primary_issue,
        comments,
    })
}

fn parse_pull_request_comments(comments_dir: &Path) -> Result<Vec<PullRequestComment>> {
    let mut comments = Vec::new();
    if comments_dir.exists() {
        for path in read_dir_sorted_paths(comments_dir)? {
            if path.is_file() && path.extension().is_some_and(|extension| extension == "md") {
                let comment = parse_pull_request_comment(&path)?;
                comments.push(comment);
            }
            warn!(
                "Issue comments directory {:?} contains non-.md file {:?}",
                comments_dir, path
            );
        }
    }

    Ok(comments)
}

fn parse_pull_request_comment(comment_path: &Path) -> Result<PullRequestComment> {
    let comment_file_content = fs::read_to_string(comment_path)?;
    let (frontmatter, content) = split_frontmatter(&comment_file_content);

    if let Some(frontmatter) = frontmatter {
        Ok(PullRequestComment {
            meta: Some(toml::from_str(frontmatter)?),
            content: content.to_string(),
        })
    } else {
        Ok(PullRequestComment {
            meta: None,
            content: comment_file_content,
        })
    }
}

fn parse_commits_dir(commits_dir: &Path) -> Result<Vec<Commit>> {
    let paths = read_dir_paths(commits_dir)?;

    let dirs = paths.iter().filter(|path| path.is_dir());
    let commits = dirs.map(|dir| {
        let txt = dir.with_extension("txt");
        if txt.is_file() {
            (dir, Some(txt))
        } else {
            (dir, None)
        }
    });

    // TODO warn about non-txt files
    // TODO warn about txt files with no corresponding directories

    let mut parsed_commits = Vec::new();
    for commit in commits {
        parsed_commits.push(Commit {
            path: commit.0.to_path_buf(),
            message: match commit.1 {
                Some(path) => Some(fs::read_to_string(path)?),
                None => None,
            },
        });
    }
    Ok(parsed_commits)
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn test_split_frontmatter() {
        let res = split_frontmatter("+++\nfront\n+++\nback");
        assert_eq!(res, (Some("front\n"), "back"));

        let res = split_frontmatter("+++\nfront\nback");
        assert_eq!(res, (None, "+++\nfront\nback"));

        let res = split_frontmatter("+++\nfront\n+++\nback\n+++\nmore");
        assert_eq!(res, (Some("front\n"), "back\n+++\nmore"));

        let res = split_frontmatter("front\n+++\nback");
        assert_eq!(res, (None, "front\n+++\nback"));

        let res = split_frontmatter("+++\n+++\nback");
        assert_eq!(res, (Some(""), "back"));

        let res = split_frontmatter("+++\n\n+++\nback");
        assert_eq!(res, (Some("\n"), "back"));

        // with leading whitespace
        let res = split_frontmatter(" +++\nfront\n+++\nback");
        assert_eq!(res, (Some("front\n"), "back"));

        let res = split_frontmatter(" front\n+++\nback");
        assert_eq!(res, (None, " front\n+++\nback"));
    }

    #[test]
    fn test_parse_primary_issue() {
        let data = r#"+++
title = "My Title"
+++
Content line 1
Content line 2
"#;
        let res = parse_primary_issue(Cursor::new(data)).unwrap();
        assert_eq!(
            res,
            PrimaryIssue {
                meta: Some(IssueMeta {
                    title: "My Title".to_string()
                }),
                content: "Content line 1\nContent line 2\n".to_string(),
            }
        );

        let data = r#"

+++
title = "My Title"
+++
Content line 1
Content line 2
"#;
        let res = parse_primary_issue(Cursor::new(data)).unwrap();
        assert_eq!(
            res,
            PrimaryIssue {
                meta: Some(IssueMeta {
                    title: "My Title".to_string()
                }),
                content: "Content line 1\nContent line 2\n".to_string(),
            }
        );

        let data = r#"
+++
title = "My Title"
+++

Content line 1
Content line 2

"#;
        let res = parse_primary_issue(Cursor::new(data)).unwrap();
        assert_eq!(
            res,
            PrimaryIssue {
                meta: Some(IssueMeta {
                    title: "My Title".to_string()
                }),
                content: "\nContent line 1\nContent line 2\n\n".to_string(),
            }
        );

        let data = r#"
Content line 1
Content line 2
"#;
        let res = parse_primary_issue(Cursor::new(data)).unwrap();
        assert_eq!(
            res,
            PrimaryIssue {
                meta: None,
                content: data.to_string(),
            }
        );

        let data = r#"
title = "My Title"
+++

Content line 1
Content line 2

"#;
        let res = parse_primary_issue(Cursor::new(data)).unwrap();
        assert_eq!(
            res,
            PrimaryIssue {
                meta: None,
                content: data.to_string(),
            }
        );

        let data = r#"
+++
title = "My Title"
author = "Me"
+++

Content line 1
Content line 2

"#;
        let res = parse_primary_issue(Cursor::new(data));
        assert!(res.is_err(), "Expected parse error.");
    }

    #[test]
    fn test_parse_issue() {
        let issue = parse_issue(&PathBuf::from("test-data/test-quest/00-first")).unwrap();
        assert_eq!(
            issue,
            Issue {
                primary_issue: PrimaryIssue {
                    meta: Some(IssueMeta {
                        title: "Warmup".to_string()
                    }),
                    content: "Issue content referencing #{{ chapter.pr }}.\n".to_string()
                },
                comments: vec![
                    "First comment on an issue\n".to_string(),
                    "Second comment on an issue.\n".to_string()
                ]
            }
        );
    }

    #[test]
    fn test_read_dir_sorted_paths() {
        let paths =
            read_dir_sorted_paths(&PathBuf::from("test-data/test-quest/00-first/issue")).unwrap();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("test-data/test-quest/00-first/issue/00-comment-about-foo.md"),
                PathBuf::from("test-data/test-quest/00-first/issue/01-other-comment.md")
            ]
        );
    }

    #[test]
    fn test_parse_commit_dir() {
        let res =
            parse_commits_dir(&PathBuf::from("test-data/test-quest/00-first/scaffold")).unwrap();
        assert_eq!(
            res,
            vec![
                Commit {
                    path: PathBuf::from(
                        "test-data/test-quest/00-first/scaffold/01-add-placeholders"
                    ),
                    message: Some(
                        "commit message for final commit in scaffold branch\n".to_string()
                    )
                },
                Commit {
                    path: PathBuf::from(
                        "test-data/test-quest/00-first/scaffold/00-prepare-interfaces"
                    ),
                    message: Some("commit message\n".to_string())
                }
            ]
        );
    }
}
