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

    pub fn continue_after(&self) -> bool {
        match self {
            TestChapterSelection::AllChapters | TestChapterSelection::FollowingChapters(_) => true,
            TestChapterSelection::OneChapter(_) => false,
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            TestChapterSelection::AllChapters => None,
            TestChapterSelection::OneChapter(name)
            | TestChapterSelection::FollowingChapters(name) => Some(name),
        }
    }
}

pub struct TestResult {
    commit: PathBuf,
    passed: bool,
    expected: bool,
}

impl std::fmt::Display for TestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.passed {
            ansi_term::Colour::Green.paint("PASSED")
        } else {
            ansi_term::Colour::Yellow.paint("FAILED")
        };
        let expected = if self.expected {
            ansi_term::Colour::Green.paint("EXPECTED RESULT")
        } else {
            ansi_term::Colour::Red.paint("UNEXPECTED RESULT")
        };
        write!(f, "{}: {} `{}`", expected, status, self.commit.display())
    }
}

impl From<(Commit, ExitStatus)> for TestResult {
    fn from(value: (Commit, ExitStatus)) -> Self {
        let (commit, status) = value;
        TestResult {
            commit: commit.path,
            passed: status.success(),
            expected: commit.expected.is_pass() == status.success(),
        }
    }
}

pub fn test_quest(
    dir: &Path,
    skip_scaffold: bool,
    chapter_selection: &TestChapterSelection,
) -> Result<()> {
    let mut all_expected = true;
    let quest = dir::parse(dir)?;
    let Some(test_cmd) = quest.test_cmd else {
        anyhow::bail!("No test command specified.");
    };

    let mut found = false;
    if chapter_selection.run_main() {
        found = true;
        for commit in quest.main {
            all_expected &= run_test(&test_cmd, commit)?;
        }
    }
    let mut keep_running = false;
    for chapter in quest.chapters {
        if keep_running || chapter_selection.is_start_chapter(&chapter.label) {
            keep_running = chapter_selection.continue_after();
            found = true;
            if !skip_scaffold {
                for commit in chapter.scaffold.into_iter().flatten() {
                    all_expected &= run_test(&test_cmd, commit)?;
                }
            }
            for commit in chapter.solution {
                all_expected &= run_test(&test_cmd, commit)?;
            }
        }
    }

    if let Some(chapter_name) = chapter_selection.name()
        && !found
    {
        anyhow::bail!("Specified chapter {chapter_name} not found.");
    }

    if all_expected {
        Ok(())
    } else {
        Err(anyhow::anyhow!("There were unexpected test failures."))
    }
}

fn run_test(cmd: &[String], commit: Commit) -> Result<bool> {
    let Some((exe, args)) = cmd.split_first() else {
        anyhow::bail!("Test command must have at least the program specified.");
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args);
    cmd.current_dir(&commit.path);
    let res = cmd.output().with_context(|| {
        format!(
            "Failed to run test command {cmd:?} for commit `{}`",
            commit.path.display()
        )
    })?;
    let res = TestResult::from((commit, res.status));
    println!("{res}");
    Ok(res.expected)
}
