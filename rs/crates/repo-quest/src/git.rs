use std::process::{Command, Stdio};

use anyhow::{ensure, Context, Result};

use crate::github::GithubRepo;

fn git_core(f: impl FnOnce(&mut Command), capture: bool) -> Result<Option<String>> {
  let mut cmd = Command::new("git");
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

fn git(f: impl FnOnce(&mut Command)) -> Result<()> {
  git_core(f, false).map(|_| ())
}

fn git_output(f: impl FnOnce(&mut Command)) -> Result<String> {
  git_core(f, true).map(|s| s.unwrap())
}

pub struct GitRepo {}

pub const UPSTREAM: &str = "upstream";

pub enum MergeType {
  CherryPick,
  HardReset,
}

impl GitRepo {
  pub fn new() -> Self {
    GitRepo {}
  }

  pub fn clone(&self, url: &str) -> Result<()> {
    git(|cmd| {
      cmd.args(["clone", url]);
    })
    .with_context(|| format!("Failed to clone: {url}"))
  }

  pub fn setup_upstream(&self, upstream: &GithubRepo) -> Result<()> {
    git(|cmd| {
      cmd.args(["remote", "add", UPSTREAM, &upstream.remote()]);
    })
    .with_context(|| format!("Failed to add upstream {}", upstream.remote()))?;

    git(|cmd| {
      cmd.args(["fetch", UPSTREAM]);
    })
    .with_context(|| format!("Failed to fetch upstream {}", upstream.remote()))?;

    Ok(())
  }

  pub fn create_branch_from(
    &self,
    target_branch: &str,
    base_branch: &str,
  ) -> Result<(String, MergeType)> {
    git(|cmd| {
      cmd.args(["checkout", "-b", target_branch]);
    })
    .with_context(|| format!("Failed to checkout branch {target_branch}"))?;

    let res = git(|cmd| {
      cmd.args([
        "cherry-pick",
        &format!("{UPSTREAM}/{base_branch}..{UPSTREAM}/{target_branch}"),
      ]);
    });

    let merge_type = match res {
      Ok(_) => MergeType::CherryPick,
      Err(e) => {
        tracing::warn!("Merge conflicts when cherry-picking, resorting to hard reset: ${e:?}");

        git(|cmd| {
          cmd.args(["cherry-pick", "--abort"]);
        })
        .context("Failed to abort cherry-pick")?;

        let upstream_target = format!("{UPSTREAM}/{target_branch}");
        git(|cmd| {
          cmd.args(["reset", "--hard", &upstream_target]);
        })
        .with_context(|| format!("Failed to hard reset to {upstream_target}"))?;

        git(|cmd| {
          cmd.args(["reset", "--soft", "main"]);
        })
        .context("Failed to soft reset to main")?;

        git(|cmd| {
          cmd.args(["commit", "-m", "Override with reference solution"]);
        })
        .context("Failed to commit reference solution")?;

        MergeType::HardReset
      }
    };

    git(|cmd| {
      cmd.args(["push", "-u", "origin", target_branch]);
    })
    .with_context(|| format!("Failed to push branch {target_branch}"))?;

    let head = self.head_commit()?;

    git(|cmd| {
      cmd.args(["checkout", "main"]);
    })
    .context("Failed to checkout main")?;

    Ok((head, merge_type))
  }

  pub fn checkout_main_and_pull(&self) -> Result<()> {
    git(|cmd| {
      cmd.args(["checkout", "main"]);
    })
    .context("Failed to checkout main")?;

    git(|cmd| {
      cmd.args(["pull"]);
    })
    .context("Failed to pull main")?;

    Ok(())
  }

  pub fn head_commit(&self) -> Result<String> {
    let output = git_output(|cmd| {
      cmd.args(["rev-parse", "HEAD"]);
    })
    .context("Failed to get head commit")?;
    Ok(output.trim_end().to_string())
  }

  pub fn reset(&self, branch: &str) -> Result<()> {
    git(|cmd| {
      cmd.args(["reset", "--hard", branch]);
    })
    .context("Failed to reset")?;

    git(|cmd| {
      cmd.args(["push", "--force"]);
    })
    .context("Failed to push reset branch")?;

    Ok(())
  }
}
