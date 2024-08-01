use std::process::{Command, Stdio};

use anyhow::{ensure, Context, Result};

use crate::github_repo::GithubRepo;

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

const UPSTREAM: &str = "upstream";

impl GitRepo {
  pub fn new() -> Self {
    GitRepo {}
  }

  pub fn clone(&self, url: &str) -> Result<()> {
    git(|cmd| {
      cmd.args(["clone", url]);
    })
    .context("Failed to clone")
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

  pub fn create_branch_from(&self, target_branch: &str, base_branch: &str) -> Result<()> {
    git(|cmd| {
      cmd.args(["checkout", "-b", target_branch]);
    })
    .with_context(|| format!("Failed to checkout branch {target_branch}"))?;

    git(|cmd| {
      cmd.args([
        "cherry-pick",
        &format!("{UPSTREAM}/{base_branch}..{UPSTREAM}/{target_branch}"),
      ]);
    })
    .with_context(|| format!("Failed to cherry-pick commits onto {target_branch}"))?;

    git(|cmd| {
      cmd.args(["push", "-u", "origin", target_branch]);
    })
    .with_context(|| format!("Failed to push branch {target_branch}"))?;

    Ok(())
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
}
