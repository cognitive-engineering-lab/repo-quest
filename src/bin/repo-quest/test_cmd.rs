use std::{
    path::{Path, PathBuf},
    process::ExitStatus,
};

use anyhow::{Context as _, Result};

use crate::dir::{self, Commit};

pub enum TestChapterSelection {
    AllChapters,
    OneChapter(String),
    FollowingChapters(String),
}

impl TestChapterSelection {
    pub fn run_main(&self) -> bool {
        match self {
            TestChapterSelection::AllChapters => true,
            TestChapterSelection::OneChapter(name) => name == "main",
            TestChapterSelection::FollowingChapters(_) => false,
        }
    }

    pub fn is_start_chapter(&self, chapter: &str) -> bool {
        match self {
            TestChapterSelection::AllChapters => true,
            TestChapterSelection::OneChapter(name) => name != "main" && name == chapter,
            TestChapterSelection::FollowingChapters(name) => name == chapter,
        }
    }
}

pub struct TestResult {
    commit: PathBuf,
    passed: bool,
    expected: bool,
}

impl From<(Commit, ExitStatus)> for TestResult {
    fn from(value: (Commit, ExitStatus)) -> Self {
        let (commit, status) = value;
        TestResult {
            commit: commit.path,
            passed: status.success(),
            expected: commit.expected_test_result.is_pass() == status.success(),
        }
    }
}

pub fn test_quest(
    dir: &Path,
    skip_scaffold: bool,
    chapter_selection: TestChapterSelection,
) -> Result<()> {
    let mut test_results = Vec::new();
    let quest = dir::parse(dir)?;
    if let Some(test_cmd) = quest.test_cmd {
        if let Some((exe, args)) = test_cmd.split_first() {
            let cmd = || {
                let mut cmd = std::process::Command::new(exe);
                cmd.args(args);
                cmd
            };
            if chapter_selection.run_main() {
                for commit in quest.main.into_iter().flatten() {
                    let res = cmd().current_dir(&commit.path).output().with_context(|| {
                        format!("Failed to run test command for commit {:?}", commit.path)
                    })?;
                    test_results.push(TestResult::from((commit, res.status)));
                }
            }
            let mut keep_running = false;
            for chapter in quest.chapters {
                if keep_running || chapter_selection.is_start_chapter(&chapter.branch_name) {
                    keep_running = true;
                    if !skip_scaffold {
                        for commit in chapter.scaffold.into_iter().flatten() {
                            let res =
                                cmd().current_dir(&commit.path).output().with_context(|| {
                                    format!(
                                        "Failed to run test command for commit {:?}",
                                        commit.path
                                    )
                                })?;
                            test_results.push(TestResult::from((commit, res.status)));
                        }
                    }
                    for commit in chapter.solution {
                        let res = cmd().current_dir(&commit.path).output().with_context(|| {
                            format!("Failed to run test command for commit {:?}", commit.path)
                        })?;
                        test_results.push(TestResult::from((commit, res.status)));
                    }
                }
            }
        } else {
            anyhow::bail!("Test command must have at least the program specified.");
        }
    } else {
        anyhow::bail!("No test command specified.");
    }

    let mut all_expected = true;
    for result in test_results {
        all_expected &= result.expected;
        let status = if result.passed {
            ansi_term::Colour::Green.paint("PASSED")
        } else {
            ansi_term::Colour::Yellow.paint("FAILED")
        };
        let expected = if result.expected {
            ansi_term::Colour::Green.paint("EXPECTED RESULT")
        } else {
            ansi_term::Colour::Red.paint("UNEXPECTED RESULT")
        };
        println!("{}: {} {:?}", expected, status, result.commit);
    }

    if all_expected {
        Ok(())
    } else {
        Err(anyhow::anyhow!("There were unexpected test failures."))
    }
}
