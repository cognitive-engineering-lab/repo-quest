use anyhow::{Context as _, Result, anyhow};
use log::debug;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Debug, Display},
    path::PathBuf,
    process::Command,
};

/// Extension trait for running commands to completion with an anyhow context
/// message on a failure exit code.
trait RunCommand {
    fn run_with_context<C, F>(&mut self, f: F) -> Result<()>
    where
        C: Display + Debug + Send + Sync + 'static,
        F: FnOnce() -> C;

    fn stdout_with_context<C, F>(&mut self, f: F) -> Result<String>
    where
        C: Display + Debug + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl RunCommand for Command {
    fn run_with_context<C, F>(&mut self, f: F) -> Result<()>
    where
        C: Display + Debug + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        if self.spawn()?.wait()?.success() {
            Ok(())
        } else {
            Err(anyhow!(f()))
        }
    }

    fn stdout_with_context<C, F>(&mut self, f: F) -> Result<String>
    where
        C: Display + Debug + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        let output = self.stdout(std::process::Stdio::piped()).output()?;
        if output.status.success() {
            let rev = String::from_utf8(output.stdout)?;
            Ok(rev)
        } else {
            Err(anyhow!(
                "Child process exited with non-success exit code {}.",
                output.status
            ))
            .with_context(f)
        }
    }
}

/// Represents a local (i.e., on the filesystem) git repository and provides
/// methods for manipulating it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitRepo {
    dir: PathBuf,
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
    pub fn init(dir: PathBuf) -> Result<GitRepo> {
        Command::new("git")
            .arg("init")
            .arg("--initial-branch")
            .arg("main")
            .arg(&dir)
            .run_with_context(|| format!("Could not initilize git repo in {dir:?}"))?;

        Ok(GitRepo { dir })
    }

    /// Adds a remote to this git repository.
    pub fn add_remote(&self, remote_name: &str, remote_url: &str) -> Result<()> {
        debug!("Adding remote named {remote_name} with url {remote_url} to {self:?}");
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
            .run_with_context(|| format!("Could not restore from {source} in {self:?}"))
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
            .arg("-c")
            .arg(branch_source)
            .arg(branch_name)
            .run_with_context(|| {
                format!(
                    "Could not create branch {branch_name} from {branch_source} for repo {self:?}."
                )
            })
    }

    pub fn switch_branch(&self, branch_name: &str) -> Result<()> {
        self.git()
            .arg("switch")
            .arg(branch_name)
            .run_with_context(|| {
                format!("Could not switch to branch {branch_name} for repo {self:?}.")
            })
    }
}
