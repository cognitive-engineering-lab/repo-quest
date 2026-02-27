use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use log::{debug, info, warn};
use regex::Regex;

use super::*;

/// Parse a directory into a [`QuestDefinition`].
///
/// See [the parent module][super] for a description of the format.
pub fn parse(dir: &Path) -> Result<QuestDefinition> {
    let quest_file_content = fs::read_to_string(dir.join("quest.txt"))?;
    let (front, description) = split_frontmatter(&quest_file_content);
    let meta: Meta =
        toml::from_str(front.context("quest.txt requires frontmatter with quest metadata.")?)?;
    let chapters = parse_chapters(dir)?;
    let main_dir = dir.join("main");
    let main = if main_dir.is_dir() {
        Some(parse_commits_dir(&main_dir)?)
    } else {
        None
    };
    Ok(QuestDefinition {
        meta,
        description: description.to_string(),
        main,
        chapters,
    })
}

fn parse_chapters(dir: &Path) -> Result<Vec<Chapter>> {
    let chapter_dirs = chapter_dirs(dir)?;
    debug!("Chapters dirs: {:?}", chapter_dirs);

    let mut chapters = Vec::with_capacity(chapter_dirs.len());
    for chapter_dir in chapter_dirs {
        chapters.push(parse_chapter(chapter_dir)?);
    }

    Ok(chapters)
}

/// Gets all potential chapter directories at the given path, sorted by name in
/// lexicographical order.
///
/// A potential chapter directory is a directory that is not named `main` and
/// that does not begin with a `.`.
fn chapter_dirs(dir: &Path) -> Result<Vec<PathBuf>, anyhow::Error> {
    let chapter_dirs: Vec<PathBuf> = read_dir_sorted_paths(dir)?
        .into_iter()
        .filter(|path| {
            path.is_dir()
                && !path.ends_with("main")
                && !path
                    .file_name()
                    .and_then(|path| path.to_str())
                    .is_some_and(|path| path.starts_with("."))
        })
        .collect();

    Ok(chapter_dirs)
}

fn parse_chapter(chapter_dir: PathBuf) -> Result<Chapter> {
    debug!("Parsing chapters dir: {chapter_dir:?}");

    let branch_name = parse_branch_name(&chapter_dir)?;
    let issue = parse_issue(&chapter_dir)?;
    let pull_request = parse_pull_request(&chapter_dir)?;
    info!("Processing chapter {chapter_dir:?}");
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

fn parse_branch_name(chapter_dir: &Path) -> Result<String, anyhow::Error> {
    Ok(chapter_dir
        .file_name()
        .with_context(|| format!("Could not extract branchname from {:?}.", chapter_dir))?
        .to_string_lossy()
        .into_owned())
}

fn parse_issue(chapter_dir: &Path) -> Result<Issue> {
    let primary_issue = parse_primary_issue(&chapter_dir.join("issue.md"))?;
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

/// If there is frontmatter, produces the parsed structure and the remaining
/// string with the frontmatter removed.
///
/// If there is no frontmatter, produces `None`.
///
/// If the frontmatter can't be parsed, produces `Err`.
fn parse_frontmatter<'a, T>(data: &'a str) -> Result<Option<(T, &'a str)>>
where
    T: Deserialize<'a>,
{
    let (frontmatter, content) = split_frontmatter(data);

    if let Some(frontmatter) = frontmatter {
        Ok(Some((toml::from_str(frontmatter)?, content)))
    } else {
        Ok(None)
    }
}

fn parse_primary_issue(issue_path: &Path) -> Result<PrimaryIssue> {
    let issue_file_content = fs::read_to_string(issue_path)
        .with_context(|| format!("Could not read issue file {issue_path:?}"))?;

    if let Some((frontmatter, content)) = parse_frontmatter(&issue_file_content)
        .with_context(|| format!("Could not parse frontmatter from {issue_path:?}"))?
    {
        Ok(PrimaryIssue {
            meta: Some(frontmatter),
            content: content.to_string(),
        })
    } else {
        Ok(PrimaryIssue {
            meta: None,
            content: issue_file_content,
        })
    }
}

fn read_dir_sorted_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = read_dir_paths(dir)?;
    paths.sort();
    Ok(paths)
}

fn read_dir_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(dir
        .read_dir()
        .with_context(|| format!("Could not read directory {dir:?}"))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("Failure while reading directory {dir:?}"))?
        .iter()
        .map(|entry| entry.path())
        .collect())
}

fn comment_files(comments_dir: &Path) -> Result<Option<Vec<PathBuf>>> {
    if comments_dir.exists() {
        let mut comment_files = Vec::new();
        for path in read_dir_sorted_paths(comments_dir)? {
            if !path.is_file() {
                warn!("Comments directory {comments_dir:?} contains non-file {path:?}");
            } else if path.extension().is_none_or(|extension| extension != "md") {
                warn!("Comments directory {comments_dir:?} contains non-.md file {path:?}");
            } else {
                comment_files.push(path);
            }
        }
        Ok(Some(comment_files))
    } else {
        Ok(None)
    }
}

fn parse_issue_comments(comments_dir: &Path) -> Result<Option<Vec<String>>> {
    if let Some(comment_files) = comment_files(comments_dir)? {
        let mut comments = Vec::with_capacity(comment_files.len());
        for path in comment_files {
            comments.push(
                fs::read_to_string(&path)
                    .with_context(|| format!("Could not read comment file {path:?}"))?,
            );
        }
        Ok(Some(comments))
    } else {
        Ok(None)
    }
}

fn parse_pull_request(chapter_dir: &Path) -> Result<PullRequest> {
    let pr_path = chapter_dir.join("pr.md");
    // PR primary issue is optional.
    let primary_issue = if pr_path.exists() {
        Some(parse_primary_issue(&pr_path)?)
    } else {
        None
    };

    let comments = parse_pull_request_comments(&chapter_dir.join("pr"))?;

    Ok(PullRequest {
        primary_issue,
        comments,
    })
}

fn parse_pull_request_comments(comments_dir: &Path) -> Result<Option<Vec<PullRequestComment>>> {
    if let Some(comment_files) = comment_files(comments_dir)? {
        let mut comments = Vec::with_capacity(comment_files.len());
        for path in comment_files {
            comments.push(parse_pull_request_comment(&path)?);
        }
        Ok(Some(comments))
    } else {
        Ok(None)
    }
}

fn parse_pull_request_comment(comment_path: &Path) -> Result<PullRequestComment> {
    let comment_file_content = fs::read_to_string(comment_path)
        .with_context(|| format!("Could not read comment file {comment_path:?}"))?;

    // TODO: validate filename in frontmatter
    if let Some((frontmatter, content)) = parse_frontmatter(&comment_file_content)
        .with_context(|| format!("Could not parse TOML frontmatter from {comment_path:?}"))?
    {
        Ok(PullRequestComment {
            meta: Some(frontmatter),
            content: content.to_string(),
        })
    } else {
        Ok(PullRequestComment {
            meta: None,
            content: comment_file_content,
        })
    }
}

/// Produces all of the commit information for a quest. Does not validate other
/// quest definition requirements, such as the presence of `issue.md`.
pub fn parse_quest_commits(dir: &Path) -> Result<QuestCommits> {
    let chapter_dirs = chapter_dirs(dir)?;
    let main_dir = dir.join("main");
    let main = if main_dir.is_dir() {
        Some(parse_commits_dir(&main_dir)?)
    } else {
        None
    };
    let mut chapters = Vec::with_capacity(chapter_dirs.len());
    for chapter_dir in chapter_dirs {
        let branch_name = parse_branch_name(&chapter_dir)?;
        let scaffold_dir = &dir.join("scaffold");
        let scaffold = if scaffold_dir.is_dir() {
            Some(parse_commits_dir(scaffold_dir)?)
        } else {
            None
        };
        let solution = parse_commits_dir(&dir.join("solution"))?;
        let chapter = ChapterCommits {
            branch_name,
            scaffold,
            solution,
        };
        chapters.push(chapter);
    }
    Ok(QuestCommits { main, chapters })
}

pub fn parse_commits_dir(commits_dir: &Path) -> Result<Vec<Commit>> {
    let paths = read_dir_sorted_paths(commits_dir)?;

    let dirs = paths.iter().filter(|path| path.is_dir());
    let commits = dirs.map(|dir| {
        let txt = dir.with_extension("txt");
        if txt.is_file() {
            (dir, Some(txt))
        } else {
            (dir, None)
        }
    });

    // TODO warn about non-.txt files
    // TODO warn about txt files with no corresponding directories

    let mut parsed_commits = Vec::new();
    for (path, message_file) in commits {
        parsed_commits.push(Commit {
            path: path.to_path_buf(),
            message: match message_file {
                Some(path) => Some(
                    fs::read_to_string(&path)
                        .with_context(|| format!("Could not open commit message file {path:?}"))?,
                ),
                None => None,
            },
        });
    }
    Ok(parsed_commits)
}

#[cfg(test)]
mod test {
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
    fn test_parse_frontmatter() {
        let data = r#"+++
title = "My Title"
+++
Content line 1
Content line 2
"#;
        let res = parse_frontmatter(data).unwrap();
        assert_eq!(
            res,
            Some((
                IssueMeta {
                    title: "My Title".to_string()
                },
                "Content line 1\nContent line 2\n"
            ))
        );

        let data = r#"

+++
title = "My Title"
+++
Content line 1
Content line 2
"#;
        let res = parse_frontmatter(data).unwrap();
        assert_eq!(
            res,
            Some((
                IssueMeta {
                    title: "My Title".to_string()
                },
                "Content line 1\nContent line 2\n"
            ))
        );

        let data = r#"
+++
title = "My Title"
+++

Content line 1
Content line 2

"#;
        let res = parse_frontmatter(data).unwrap();
        assert_eq!(
            res,
            Some((
                IssueMeta {
                    title: "My Title".to_string()
                },
                "\nContent line 1\nContent line 2\n\n"
            ))
        );

        let data = r#"
Content line 1
Content line 2
"#;
        let res = parse_frontmatter::<IssueMeta>(data).unwrap();
        assert_eq!(res, None);

        let data = r#"
title = "My Title"
+++

Content line 1
Content line 2

"#;
        let res = parse_frontmatter::<IssueMeta>(data).unwrap();
        assert_eq!(res, None);

        let data = r#"
+++
title = "My Title"
author = "Me"
+++

Content line 1
Content line 2

"#;
        let res = parse_frontmatter::<IssueMeta>(data);
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
                comments: Some(vec![
                    "First comment on an issue\n".to_string(),
                    "Second comment on an issue.\n".to_string()
                ])
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
    fn test_parse_commits_dir() {
        let res =
            parse_commits_dir(&PathBuf::from("test-data/test-quest/00-first/scaffold")).unwrap();
        assert_eq!(
            res,
            vec![
                Commit {
                    path: PathBuf::from(
                        "test-data/test-quest/00-first/scaffold/00-prepare-interfaces"
                    ),
                    message: Some("commit message\n".to_string())
                },
                Commit {
                    path: PathBuf::from(
                        "test-data/test-quest/00-first/scaffold/01-add-placeholders"
                    ),
                    message: Some(
                        "commit message for final commit in scaffold branch\n".to_string()
                    )
                }
            ]
        );
    }
}
