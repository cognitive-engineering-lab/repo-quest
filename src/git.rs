use std::{
  collections::HashMap,
  fs,
  io::Write,
  path::{Path, PathBuf},
  process::Stdio,
};

use eyre::{Context, Result, ensure, eyre};

use crate::{
  command::command,
  github::{GitProtocol, GithubRepo},
  package::QuestPackage,
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

  pub fn exists(&self) -> bool {
    let output = git_output!(self, "rev-parse --is-inside-work-tree");
    match output {
      Ok(stdout) => stdout.trim() == "true",
      Err(_) => false,
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

  fn git_core(&self, args: &str) -> Result<std::result::Result<String, String>> {
    let mut cmd = command(&format!("git {args}"), &self.path);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd.output()?;
    if !output.status.success() {
      return Ok(Err(String::from_utf8(output.stderr)?));
    }

    let stdout = String::from_utf8(output.stdout)?;
    Ok(Ok(stdout))
  }

  fn git(&self, args: &str) -> Result<()> {
    self.git_output(args)?;
    Ok(())
  }

  fn git_output(&self, args: &str) -> Result<String> {
    self
      .git_core(args)?
      .map_err(|stderr| eyre!("git failed with stderr:\n{stderr}"))
  }

  pub fn setup_upstream(&self, upstream: &GithubRepo) -> Result<()> {
    let remote = upstream.remote(GitProtocol::Https);
    git!(self, "remote add {UPSTREAM} {remote}")?;
    self.fetch(UPSTREAM)?;
    Ok(())
  }

  pub fn fetch(&self, remote: &str) -> Result<()> {
    git!(self, "fetch {remote}")
  }

  pub fn upstream(&self) -> Result<Option<&'static str>> {
    let status = command(&format!("git remote get-url {UPSTREAM}"), &self.path)
      .stdout(Stdio::null())
      .status()
      .context("`git remote` failed")?;
    Ok(status.success().then_some(UPSTREAM))
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

  fn first_commit_off_main(&self, branch: &str) -> Result<String> {
    let merge_base =
      git_output!(self, "merge-base main {branch}").context("Failed to get merge-base")?;
    let merge_base = merge_base.trim();
    let commits = git_output!(self, "rev-list --ancestry-path {merge_base}..{branch}")
      .context("Failed to get rev-list")?;
    Ok(commits.lines().last().unwrap().to_string())
  }

  fn current_branch(&self) -> Result<String> {
    let output = git_output!(self, "rev-parse --abbrev-ref HEAD")?;
    Ok(output.trim().to_string())
  }

  pub fn cherry_pick(&self, from: &str, to: &str) -> Result<MergeType> {
    let res = git!(self, "cherry-pick {from}..{to}");

    match res {
      Ok(()) => Ok(MergeType::Success),
      Err(e) => {
        tracing::warn!("Merge conflicts when cherry-picking, resorting to hard reset: ${e:?}");

        git!(self, "cherry-pick --abort").context("Failed to abort cherry-pick")?;

        let cur_branch = self.current_branch()?;
        let first_commit = self.first_commit_off_main(&cur_branch)?;

        git!(self, "reset --hard {to}")?;
        git!(self, "reset --soft origin/{cur_branch}")?;
        git!(self, "checkout {first_commit} README.md")?;
        git!(self, "commit -m 'Hard reset to reference solution'")?;

        // git!(self, "revert --no-commit {first_commit}..")?;
        // git!(self, "commit -m 'Revert conflicting user changes'")?;

        // git!(self, "cherry-pick {from}..{to}")
        //   .context("Clean cherry-pick failed, catastrophic git situation")?;

        Ok(MergeType::SolutionReset)
      }
    }
  }

  pub fn create_branch(&self, branch: &str) -> Result<()> {
    git!(self, "checkout -b {branch}")
  }

  pub fn create_commit(&self, message: &str) -> Result<()> {
    git!(self, "add -u")?;
    git!(self, "commit -m {}", shell_escape::escape(message.into()))
  }

  pub fn push_branch(&self, branch: &str) -> Result<()> {
    git!(self, "push -u origin {branch}")
  }

  pub fn pull(&self) -> Result<()> {
    git!(self, "pull")
  }

  pub fn checkout_main(&self) -> Result<()> {
    git!(self, "checkout main")
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

  pub fn contains_file(&self, branch: &str, file: &str) -> Result<bool> {
    let output = command(&format!("git cat-file -e {branch}:{file}"), &self.path)
      .output()
      .with_context(|| format!("Failed to `git cat-file -e {branch}:{file}`"))?;
    Ok(output.status.success())
  }

  pub fn read_file(&self, branch: &str, file: &str) -> Result<String> {
    git_output!(self, "cat-file -p {branch}:{file}")
  }

  pub fn show_bin(&self, branch: &str, file: &str) -> Result<Vec<u8>> {
    let output = command(&format!("git cat-file -p {branch}:{file}"), &self.path)
      .output()
      .with_context(|| format!("Failed to `git cat-file -p {branch}:{file}"))?;
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
        let contents = self.read_file("main", file)?;
        Ok((path, contents))
      })
      .collect()
  }

  pub fn is_behind_origin(&self) -> Result<bool> {
    let out = git_output!(self, "rev-list --count main..origin/main")?;
    let count = out
      .trim()
      .parse::<i32>()
      .with_context(|| format!("rev-list returned non-numeric output:\n{out}"))?;
    Ok(count > 0)
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
