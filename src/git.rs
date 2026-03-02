use anyhow::{Context as _, Result, bail};
use log::debug;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::Debug,
    io::Read,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::command::RunCommand as _;

/// Represents a local (i.e., on the filesystem) git repository and provides
/// methods for manipulating it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitRepo {
    pub dir: PathBuf,
}

impl GitRepo {
    /// Runs git in the appropraite working directory for manipulating this
    /// repository.
    fn git(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.dir);
        cmd
    }

    /// Creates a `GitRepo` for an existing git repository.
    pub fn open(dir: PathBuf) -> Result<GitRepo> {
        // Returns an error code if path is not inside a git repo.
        let git_repo = GitRepo { dir };
        git_repo
            .git()
            .arg("rev-parse")
            .arg("--is-inside-work-tree")
            .run_with_context(|| format!("Could not open git repo at {:?}.", git_repo.dir))?;

        Ok(git_repo)
    }

    /// Initializes a new git repository and returns a `GitRepo` representing it.
    pub fn init_bare(dir: PathBuf) -> Result<GitRepo> {
        Command::new("git")
            .arg("init")
            .arg("--bare")
            .arg(&dir)
            .run_with_context(|| format!("Could not initilize git repo in {dir:?}."))?;

        Ok(GitRepo { dir })
    }

    /// Initializes a new git repository and returns a `GitRepo` representing it.
    pub fn init(dir: PathBuf) -> Result<GitRepo> {
        Command::new("git")
            .arg("init")
            .arg("--initial-branch")
            .arg("main")
            .arg(&dir)
            .run_with_context(|| format!("Could not initilize git repo in {dir:?}."))?;

        Ok(GitRepo { dir })
    }

    /// Adds a remote to this git repository.
    pub fn add_remote(&self, remote_name: &str, remote_url: &str) -> Result<()> {
        debug!("Adding remote named {remote_name} with url {remote_url} to {self:?}.");
        self.git()
            .arg("remote")
            .arg("add")
            .arg(remote_name)
            .arg(remote_url)
            .run_with_context(|| format!("Could not add remote {remote_url:?} to {self:?}."))
    }

    /// Fetches the named remote for this git repository.
    pub fn fetch(&self, remote_name: &str) -> Result<()> {
        self.git()
            .arg("fetch")
            .arg(remote_name)
            .run_with_context(|| format!("Could not fetch remote {remote_name} in {self:?}."))
    }

    pub fn restore_from(&self, source: &str) -> Result<()> {
        self.git()
            .arg("restore")
            .arg("--source")
            .arg(source)
            .arg("--worktree")
            .arg("--staged")
            .arg(".")
            .run_with_context(|| format!("Could not restore from {source} in {self:?}."))
    }

    /// Squash merges the given source reference into the current branch. Uses
    /// `theirs` to resolve conflicts with the `ort` merge strategy.
    pub fn squash_merge(&self, source: &str) -> Result<()> {
        self.git()
            .arg("merge")
            .arg("--ff") // TODO: not needed in docker container
            .arg("--squash")
            .arg("--strategy")
            .arg("ort")
            .arg("--strategy-option")
            .arg("theirs")
            .arg(source)
            .run_with_context(|| format!("Failed to merge branch {source} in {self:?}."))
    }

    /// Creates a commit with the given message.
    pub fn commit(&self, msg: &str) -> Result<()> {
        self.git()
            .arg("commit")
            .arg("--allow-empty")
            .arg("-m")
            .arg(msg)
            .run_with_context(|| format!("Could not create commit for {self:?}."))
    }

    pub fn push(&self, remote_name: &str, local_branch: &str, remote_branch: &str) -> Result<()> {
        self.git()
            .arg("push")
            .arg(remote_name)
            .arg(format!("{local_branch}:{remote_branch}"))
            .run_with_context(|| {
                format!("Could not push to remote {remote_name} for repo {self:?}.")
            })
    }

    pub fn create_branch(&self, branch_source: &str, branch_name: &str) -> Result<()> {
        self.git()
            .arg("branch")
            .arg("--no-track")
            .arg(branch_name)
            .arg(branch_source)
            .run_with_context(|| {
                format!(
                    "Could not create branch {branch_name} from {branch_source} for repo {self:?}."
                )
            })
    }

    pub fn create_tracking_branch(&self, branch_source: &str, branch_name: &str) -> Result<()> {
        self.git()
            .arg("branch")
            .arg(branch_name)
            .arg(branch_source)
            .run_with_context(|| {
                format!(
                    "Could not create branch {branch_name} from {branch_source} for repo {self:?}."
                )
            })
    }

    /// Switch to an existing branch.
    pub fn switch_branch(&self, branch_name: &str) -> Result<()> {
        self.git()
            .arg("switch")
            .arg(branch_name)
            .run_with_context(|| {
                format!("Could not switch to branch {branch_name} for repo {self:?}.")
            })
    }

    pub fn rev_parse(&self, rev: &str) -> Result<String> {
        self.git()
            .arg("rev-parse")
            .arg(rev)
            .line_with_context(|| format!("Could not parse rev {rev} for repo {self:?}."))
    }

    pub fn cat_blob(&self, oid: &str) -> Result<String> {
        self.git()
            .arg("cat-file")
            .arg("blob")
            .arg(oid)
            .stdout_with_context(|| {
                format!("Could not cat blob for object {oid} in repo {self:?}.")
            })
    }

    /// Creates a commit with the same content as the previous and with the
    /// given message on the given branch.
    ///
    /// Unlike `commit` this works in a bare repo. Unlike `commit`, this can't
    /// create an initial empty commit.
    pub fn create_empty_commit(&self, branch: &str, message: &str) -> Result<()> {
        let tree_oid = self
            .git()
            .arg("rev-parse")
            .arg(format!("{branch}^{{tree}}"))
            .line_with_context(|| format!("Get tree oid for {branch} in {self:?}."))?;

        let parent_oid = self.rev_parse(branch)?;

        let commit_oid = self.git()
            .arg("commit-tree")
            .arg("-m")
            .arg(message)
            .arg("-p")
            .arg(&parent_oid)
            .arg(&tree_oid)
            .line_with_context(|| format!("Could not create commit for {self:?} tree object {tree_oid} with parent {parent_oid} and message {message:?}."))?;

        self.git()
            .arg("update-ref")
            .arg(format!("refs/heads/{branch}"))
            .arg(&commit_oid)
            .arg(&parent_oid)
            .run_with_context(|| {
                format!(
                    "Could not update ref refs/heads/{branch} to {commit_oid} with parent {parent_oid}."
                )
            })?;

        Ok(())
    }

    /// Copies the history from `from` to the current HEAD onto `onto` via a rebase.
    /// Preserves empty commits and merges and resolves conflicts in favor of
    /// the branch being rebased.
    pub fn rebase(&self, onto: &str, from: &str) -> Result<HashMap<String, String>> {
        let tmpfile = tempfile::NamedTempFile::new()
            .context("Could not create tempfile for storing rebase info.")?;
        let rebase_result =
            self.git()
                .arg("rebase")
                .arg("--exec")
                .arg(format!(
                    "cp .git/rebase-merge/rewritten-list {}",
                    tmpfile.path().to_str().with_context(
                        || "Could not convert tempfile path to string. {tempfile:?}."
                    )?
                ))
                .arg("--empty=keep")
                .arg("--strategy=ort")
                .arg("--strategy-option=theirs")
                .arg("--rebase-merges")
                .arg("--no-update-refs")
                .arg("--onto")
                .arg(onto)
                .arg(from)
                .run_with_context(|| format!("Could not rebase --onto={onto} {from} in {self:?}."));
        match rebase_result {
            Ok(()) => {
                // This is a hack to preserve the mapping between old an replayed commits during the rebase.
                // The rewritten-list is not a documented part of git.
                //
                // See https://stackoverflow.com/a/78997351
                let mut rewritten_commits_buf = String::new();
                let mut hashes = HashMap::new();
                tmpfile
                    .as_file()
                    .read_to_string(&mut rewritten_commits_buf)?;
                for rewrite in rewritten_commits_buf.lines() {
                    let Some(from_commit) = rewrite.split_ascii_whitespace().next() else {
                        bail!("Malformed rewritten-list from rebase in {self:?}.");
                    };
                    let Some(to_commit) = rewrite.split_ascii_whitespace().next() else {
                        bail!("Malformed rewritten-list from rebase in {self:?}.");
                    };
                    hashes.insert(from_commit.to_string(), to_commit.to_string());
                }
                Ok(hashes)
            }
            Err(err) => {
                self.git()
                    .arg("rebase")
                    .arg("--abort")
                    .run_with_context(|| {
                        format!("Could not abort failed rebase --onto={onto} {from} in {self:?}.")
                    })?;
                Err(err)
            }
        }
    }

    /// Hard reset (i.e., update the branch) the current branch to the given branch.
    pub fn hard_reset(&self, branch: &str) -> Result<()> {
        self.git()
            .arg("reset")
            .arg("--hard")
            .arg(branch)
            .run_with_context(|| format!("Could not reset to {branch} in {self:?}."))
    }

    pub fn add_all(&self) -> Result<()> {
        self.git()
            .arg("add")
            .arg(".")
            .run_with_context(|| format!("Could not add all files in {self:?}."))
    }

    pub fn switch_orphan_branch(&self, branch_name: &str) -> Result<()> {
        self.git()
            .arg("switch")
            .arg("--orphan")
            .arg(branch_name)
            .run_with_context(|| {
                format!("Could not switch to new orphan branch {branch_name} for repo {self:?}.")
            })
    }

    pub fn make_bare(&self) -> Result<()> {
        self.git()
            .arg("config")
            .arg("core.bare")
            .arg("true")
            .run_with_context(|| format!("Could not convert repo to bare repo for {self:?}."))
    }

    pub fn archive(&self, gitref: &str, output: &Path) -> Result<()> {
        self.git()
            .arg("archive")
            .arg(gitref)
            .arg("--output")
            .arg(output)
            .run_with_context(|| format!("Could not archive {self:?} ref {gitref} to {output:?}."))
    }

    pub fn copy_tree(&self, gitref: &str, output: &Path) -> Result<()> {
        let git = self
            .git()
            .arg("archive")
            .arg("--format")
            .arg("tar")
            .arg(gitref)
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("Could not archive {self:?} ref {gitref}."))?;

        Command::new("tar")
            .current_dir(&self.dir)
            .stdin(Stdio::from(git.stdout.unwrap()))
            .arg("-C")
            .arg(output)
            .arg("-x")
            .run_with_context(|| format!("Could not untar archive of {self:?} to {output:?}."))
    }

    /// git diff --quiet && git diff --cached --quiet
    pub fn has_changes(&self) -> Result<bool> {
        Ok(!self
            .git()
            .arg("diff")
            .arg("--quiet")
            .output()
            .with_context(|| format!("Could not run git diff in {self:?}."))?
            .status
            .success()
            || !self
                .git()
                .arg("diff")
                .arg("--cached")
                .arg("--quiet")
                .output()
                .with_context(|| format!("Could not run git diff --cached in {self:?}."))?
                .status
                .success())
    }
}
