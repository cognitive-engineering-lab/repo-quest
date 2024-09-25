use std::{
  collections::HashMap,
  fs,
  io::Write,
  path::{Path, PathBuf},
  process::Stdio,
};

use anyhow::{ensure, Context, Result};

use crate::{
  command::command,
  github::{GitProtocol, GithubRepo},
  package::QuestPackage,
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
    tracing::debug!("git: {arg}");
    $self.git(&arg).with_context(|| format!("git failed: {arg}"))
  }}
}

macro_rules! git_output {
  ($self:expr, $($arg:tt)*) => {{
    let arg = format!($($arg)*);
    tracing::debug!("git: {arg}");
    $self.git_output(&arg).with_context(|| format!("git failed: {arg}"))
  }}
}

impl GitRepo {
  pub fn new(path: &Path) -> Self {
    GitRepo {
      path: path.to_path_buf(),
    }
  }

  pub fn clone(path: &Path, url: &str) -> Result<Self> {
    let output = command(&format!("git clone {url}"), path.parent().unwrap()).output()?;
    ensure!(
      output.status.success(),
      "`git clone {url}` failed, stderr:\n{}",
      String::from_utf8(output.stderr)?
    );
    Ok(GitRepo::new(path))
  }

  fn git_core(&self, args: &str, capture: bool) -> Result<Option<String>> {
    let mut cmd = command(&format!("git {args}"), &self.path);
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

  fn git(&self, args: &str) -> Result<()> {
    self.git_core(args, false).map(|_| ())
  }

  fn git_output(&self, args: &str) -> Result<String> {
    self.git_core(args, true).map(|s| s.unwrap())
  }

  pub fn setup_upstream(&self, upstream: &GithubRepo) -> Result<()> {
    let remote = upstream.remote(GitProtocol::Https);
    git!(self, "remote add {UPSTREAM} {remote}")?;
    git!(self, "fetch {UPSTREAM}")?;
    Ok(())
  }

  pub fn has_upstream(&self) -> Result<bool> {
    let status = command(&format!("git remote get-url {UPSTREAM}"), &self.path)
      .status()
      .context("`git remote` failed")?;
    Ok(status.success())
  }

  fn apply(&self, patch: &str) -> Result<()> {
    tracing::trace!("Applying patch:\n{patch}");
    let mut child = command("git apply -", &self.path)
      .stdin(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(patch.as_bytes())?;
    drop(stdin);
    let output = child.wait_with_output()?;
    ensure!(
      output.status.success(),
      "git apply failed with stderr:\n{}",
      String::from_utf8(output.stderr)?
    );
    tracing::trace!("wtf: {}", String::from_utf8(output.stderr)?);
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
        git!(self, "reset --hard {upstream_target}")?;

        git!(self, "reset --soft main").context("Failed to soft reset to main")?;

        git!(self, "commit -m 'Override with reference solution'")?;

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
    git!(self, "checkout -b {target_branch}")?;

    let merge_type = template.apply_patch(self, base_branch, target_branch)?;

    git!(self, "push -u origin {target_branch}")?;

    let head = self.head_commit()?;

    git!(self, "checkout main")?;

    Ok((head, merge_type))
  }

  pub fn checkout_main_and_pull(&self) -> Result<()> {
    git!(self, "checkout main")?;
    git!(self, "pull")?;
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
  }

  pub fn show(&self, branch: &str, file: &str) -> Result<String> {
    git_output!(self, "show {branch}:{file}")
  }

  pub fn show_bin(&self, branch: &str, file: &str) -> Result<Vec<u8>> {
    let output = command(&format!("git show {branch}:{file}"), &self.path)
      .output()
      .with_context(|| format!("Failed to `git show {branch}:{file}"))?;
    ensure!(
      output.status.success(),
      "git show failed with stderr:\n{}",
      String::from_utf8(output.stderr)?
    );
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

    // HACK:Eventually we should either directly package a git repo in the file
    // or include the permissions
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let hooks_dir = self.path.join(".githooks");
      if hooks_dir.exists() {
        let hooks = fs::read_dir(&hooks_dir)
          .with_context(|| format!("Failed to read hooks directory: {}", hooks_dir.display()))?;
        for hook in hooks {
          let hook = hook.context("Failed to read hooks directory entry")?;
          let mut perms = hook
            .metadata()
            .with_context(|| format!("Failed to read hook metadata: {}", hook.path().display()))?
            .permissions();
          perms.set_mode(perms.mode() | 0o111);
          fs::set_permissions(hook.path(), perms).with_context(|| {
            format!("Failed to set hook permissions: {}", hook.path().display())
          })?;
        }
      }
    }

    git!(self, "add .")?;
    git!(self, "commit -m 'Initial commit'")?;
    git!(self, "tag {INITIAL_TAG}")?;
    git!(self, "push -u origin main")?;

    git!(self, "checkout -b meta")?;

    let config_str =
      toml::to_string_pretty(&package.config).context("Failed to parse package config")?;
    let toml_path = self.path.join("rqst.toml");
    fs::write(&toml_path, config_str)
      .with_context(|| format!("Failed to write TOML to: {}", toml_path.display()))?;

    let pkg_path = self.path.join("package.json.gz");
    package
      .save(&pkg_path)
      .with_context(|| format!("Failed to write package to: {}", pkg_path.display()))?;

    git!(self, "add .")?;
    git!(self, "commit -m 'Add meta'")?;
    git!(self, "push -u origin meta")?;
    git!(self, "checkout main")?;

    Ok(())
  }

  pub fn install_hooks(&self) -> Result<()> {
    let hooks_dir = self.path.join(".githooks");
    if hooks_dir.exists() {
      let post_checkout = hooks_dir.join("post-checkout");
      if post_checkout.exists() {
        let status = command(&post_checkout.display().to_string(), &self.path)
          .status()
          .context("post-checkout hook failed")?;
        ensure!(status.success(), "post-checkout hook failed");
      }

      git!(self, "config --local core.hooksPath .githooks")?;
    }

    Ok(())
  }
}
