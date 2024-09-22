use std::{
  collections::HashMap,
  fs,
  io::Write,
  path::{Path, PathBuf},
  process::{Command, Stdio},
};

use anyhow::{ensure, Context, Result};

use crate::{
  github::{GitProtocol, GithubRepo},
  package::QuestPackage,
  quest::QuestConfig,
  template::QuestTemplate,
};

pub struct GitRepo {
  path: PathBuf,
}

pub const UPSTREAM: &str = "upstream";
pub const INITIAL_TAG: &str = "initial";

pub enum MergeType {
  Success,
  SolutionReset,
  StarterReset,
}

macro_rules! git {
  ($self:expr, $($arg:tt)*) => {{
    let arg = format!($($arg)*);
    $self.git(|cmd| {
      tracing::debug!($($arg)*);
      cmd.args(shlex::split(&arg).unwrap());
    }).with_context(|| arg)
  }}
}

macro_rules! git_output {
  ($self:expr, $($arg:tt)*) => {{
    $self.git_output(|cmd| {
      cmd.args(shlex::split(&format!($($arg)*)).unwrap());
    })
  }}
}

impl GitRepo {
  pub fn new(path: &Path) -> Self {
    GitRepo {
      path: path.to_path_buf(),
    }
  }

  fn git_core(&self, f: impl FnOnce(&mut Command), capture: bool) -> Result<Option<String>> {
    let mut cmd = Command::new("git");
    cmd.current_dir(&self.path);
    f(&mut cmd);
    cmd.stderr(Stdio::piped());
    if capture {
      cmd.stdout(Stdio::piped());
    }

    let output = cmd.output()?;
    ensure!(
      output.status.success(),
      "git failed with stderr:\n{}",
      String::from_utf8(output.stderr)?
    );

    let stdout = if capture {
      Some(String::from_utf8(output.stdout)?)
    } else {
      None
    };

    Ok(stdout)
  }

  fn git(&self, f: impl FnOnce(&mut Command)) -> Result<()> {
    self.git_core(f, false).map(|_| ())
  }

  fn git_output(&self, f: impl FnOnce(&mut Command)) -> Result<String> {
    self.git_core(f, true).map(|s| s.unwrap())
  }

  pub fn setup_upstream(&self, upstream: &GithubRepo) -> Result<()> {
    let remote = upstream.remote(GitProtocol::Https);
    git!(self, "remote add {UPSTREAM} {remote}",)
      .with_context(|| format!("Failed to add upstream {remote}",))?;
    git!(self, "fetch {UPSTREAM}").context("Failed to fetch upstream")?;
    Ok(())
  }

  pub fn has_upstream(&self) -> Result<bool> {
    let status = Command::new("git")
      .args(["remote", "get-url", UPSTREAM])
      .current_dir(&self.path)
      .status()
      .context("`git remote` failed")?;
    Ok(status.success())
  }

  fn apply(&self, patch: &str) -> Result<()> {
    tracing::trace!("Applying patch:\n{patch}");
    let mut cmd = Command::new("git");
    cmd
      .args(["apply", "-"])
      .current_dir(&self.path)
      .stdin(Stdio::piped());
    let mut child = cmd.spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(patch.as_bytes())?;
    drop(stdin);
    let status = child.wait()?;
    ensure!(status.success(), "git apply failed");
    Ok(())
  }

  pub fn apply_patch(&self, patches: &[&str]) -> Result<MergeType> {
    let last = patches.last().unwrap();
    let merge_type = match self.apply(last) {
      Ok(()) => MergeType::Success,
      Err(e) => {
        tracing::warn!("Failed to apply patch: {e:?}");
        git!(self, "reset --hard {INITIAL_TAG}")?;
        for patch in patches {
          self.apply(patch)?;
        }
        MergeType::StarterReset
      }
    };

    git!(self, "add .")?;
    git!(self, "commit -m 'Starter code'")?;

    Ok(merge_type)
  }

  pub fn cherry_pick(&self, base_branch: &str, target_branch: &str) -> Result<MergeType> {
    let res = git!(
      self,
      "cherry-pick {UPSTREAM}/{base_branch}..{UPSTREAM}/{target_branch}"
    );

    Ok(match res {
      Ok(_) => MergeType::Success,
      Err(e) => {
        tracing::warn!("Merge conflicts when cherry-picking, resorting to hard reset: ${e:?}");

        git!(self, "cherry-pick --abort").context("Failed to abort cherry-pick")?;

        let upstream_target = format!("{UPSTREAM}/{target_branch}");
        git!(self, "reset --hard {upstream_target}")
          .with_context(|| format!("Failed to hard reset to {upstream_target}"))?;

        git!(self, "reset --soft main").context("Failed to soft reset to main")?;

        git!(self, "commit -m 'Override with reference solution'")
          .context("Failed to commit reference solution")?;

        MergeType::SolutionReset
      }
    })
  }

  pub fn create_branch_from(
    &self,
    template: &dyn QuestTemplate,
    base_branch: &str,
    target_branch: &str,
  ) -> Result<(String, MergeType)> {
    git!(self, "checkout -b {target_branch}")
      .with_context(|| format!("Failed to checkout branch {target_branch}"))?;

    let merge_type = template.apply_patch(self, base_branch, target_branch)?;

    git!(self, "push -u origin {target_branch}")
      .with_context(|| format!("Failed to push branch {target_branch}"))?;

    let head = self.head_commit()?;

    git!(self, "checkout main").context("Failed to checkout main")?;
    Ok((head, merge_type))
  }

  pub fn checkout_main_and_pull(&self) -> Result<()> {
    git!(self, "checkout main").context("Failed to checkout main")?;
    git!(self, "pull").context("Failed to pull main")?;
    Ok(())
  }

  pub fn head_commit(&self) -> Result<String> {
    let output = git_output!(self, "rev-parse HEAD").context("Failed to get head commit")?;
    Ok(output.trim_end().to_string())
  }

  pub fn reset(&self, branch: &str) -> Result<()> {
    git!(self, "reset --hard {branch}").context("Failed to reset")?;
    git!(self, "push --force").context("Failed to push reset branch")?;
    Ok(())
  }

  pub fn diff(&self, base: &str, head: &str) -> Result<String> {
    git_output!(self, "diff {base}..{head}")
      .with_context(|| format!("Failed to `git diff {base}..{head}"))
  }

  pub fn show(&self, branch: &str, file: &str) -> Result<String> {
    git_output!(self, "show {branch}:{file}")
      .with_context(|| format!("Failed to `git show {branch}:{file}"))
  }

  pub fn show_bin(&self, branch: &str, file: &str) -> Result<Vec<u8>> {
    let output = Command::new("git")
      .args(["show", &format!("{branch}:{file}")])
      .output()
      .with_context(|| format!("Failed to `git show {branch}:{file}"))?;
    Ok(output.stdout)
  }

  pub fn read_initial_files(&self) -> Result<HashMap<PathBuf, String>> {
    let ls_tree_out = git_output!(self, "ls-tree -r main --name-only")?;
    let files = ls_tree_out.trim().split("\n");
    files
      .map(|file| {
        let path = PathBuf::from(file);
        let contents = self.show("main", file)?;
        Ok((path, contents))
      })
      .collect()
  }

  pub fn write_initial_files(&self, package: &QuestPackage) -> Result<()> {
    for (rel_path, contents) in &package.initial {
      let abs_path = self.path.join(rel_path);
      if let Some(dir) = abs_path.parent() {
        fs::create_dir_all(dir)
          .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
      }
      fs::write(&abs_path, contents)
        .with_context(|| format!("Failed to write: {}", abs_path.display()))?;
    }
    git!(self, "add .")?;
    git!(self, "commit -m 'Initial commit'")?;
    git!(self, "tag {INITIAL_TAG}")?;
    git!(self, "push -u origin main")?;

    git!(self, "checkout -b meta")?;
    package.save(&self.path.join("package.json.gz"))?;
    git!(self, "add .")?;
    git!(self, "commit -m 'Add package'")?;
    git!(self, "push -u origin meta")?;
    git!(self, "checkout main")?;

    Ok(())
  }

  pub fn write_config(&self, config: &QuestConfig) -> Result<()> {
    git!(self, "checkout meta")?;
    let config_str = toml::to_string_pretty(&config)?;
    fs::write(self.path.join("rqst.toml"), config_str)?;
    git!(self, "add .")?;
    git!(self, "commit -m 'Add config'")?;
    git!(self, "push -u origin meta")?;
    git!(self, "checkout main")?;
    Ok(())
  }
}
