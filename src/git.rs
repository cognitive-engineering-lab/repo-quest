use std::{
  cell::OnceCell,
  collections::HashMap,
  fmt, fs,
  io::Write,
  path::{Path, PathBuf},
  process::{Command, Stdio},
};

use eyre::{Context, Result, bail, ensure, eyre};
use serde::{Deserialize, Serialize};

use crate::{command::command, package::QuestPackage};

pub struct GitRepo {
  path: PathBuf,
  upstream: OnceCell<Option<&'static str>>,
}

const UPSTREAM_REMOTE: &str = "upstream";
const INITIAL_TAG: &str = "initial";

pub enum MergeType {
  Success,
  Reset,
}

impl MergeType {
  pub fn is_reset(&self) -> bool {
    matches!(self, MergeType::Reset)
  }
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

macro_rules! string_newtype {
  ($name:ident) => {
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
    pub struct $name(String);

    impl $name {
      pub fn new(s: impl Into<String>) -> Self {
        $name(s.into())
      }

      pub fn as_str(&self) -> &str {
        &self.0
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
      }
    }
  };
}

string_newtype!(Branch);
string_newtype!(Ref);

impl Branch {
  pub fn main() -> Self {
    Branch::new("main")
  }

  pub fn meta() -> Self {
    Branch::new("meta")
  }
}

impl From<&Branch> for Ref {
  fn from(value: &Branch) -> Self {
    Ref::new(&value.0)
  }
}

impl GitRepo {
  pub fn new(path: &Path) -> Self {
    GitRepo {
      path: path.to_path_buf(),
      upstream: OnceCell::new(),
    }
  }

  fn git_command(&self, args: &str) -> Command {
    let mut cmd = command(&format!("git {args}"), &self.path);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd
  }

  fn git_core(&self, args: &str) -> Result<std::result::Result<String, String>> {
    let output = self.git_command(args).output()?;
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

  /// Returns true if the directory is within a git
  pub fn exists(&self) -> bool {
    let output = git_output!(self, "rev-parse --is-inside-work-tree");
    match output {
      Ok(stdout) => stdout.trim() == "true",
      Err(_) => false,
    }
  }

  /// Clones repo from `url` into the parent of `path`.
  pub fn clone(path: &Path, url: &str) -> Result<Self> {
    let repo_dir = path.parent().expect("Repo path is somehow root");
    let output = command(&format!("git clone {url}"), repo_dir).output()?;
    ensure!(
      output.status.success(),
      "`git clone {url}` failed, stderr:\n{}",
      String::from_utf8(output.stderr)?
    );
    Ok(GitRepo::new(path))
  }

  /// Add and fetch an upstream repo.
  pub fn setup_upstream(&self, remote: &str) -> Result<()> {
    git!(self, "remote add {UPSTREAM_REMOTE} {remote}")?;
    git!(self, "fetch {UPSTREAM_REMOTE}")?;
    Ok(())
  }

  /// Returns the repo's upstream remote, if it exists.
  pub fn upstream(&self) -> Result<Option<&'static str>> {
    match self.upstream.get() {
      Some(upstream) => Ok(*upstream),
      None => {
        let status = self
          .git_command(&format!("remote get-url {UPSTREAM_REMOTE}"))
          .status()
          .context("`git remote` failed")?;
        let upstream = status.success().then_some(UPSTREAM_REMOTE);
        self
          .upstream
          .set(upstream)
          .expect("GitRepo::upstream already initialized");
        Ok(upstream)
      }
    }
  }

  fn apply_patch(&self, patch: &str) -> Result<()> {
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

    Ok(())
  }

  /// Given a list of patches, attempts to apply the last patch in the list.
  /// If the application fails, then hard resets the repo back to its initial state,
  /// and applies all patches in succession.
  pub fn apply_patch_with_fallback(&self, patches: &[&str]) -> Result<MergeType> {
    let last = patches.last().unwrap();
    let merge_type = match self.apply_patch(last) {
      Ok(()) => MergeType::Success,
      Err(e) => {
        tracing::warn!("Failed to apply patch: {e:?}");
        git!(self, "reset --hard {INITIAL_TAG}")?;
        for patch in patches {
          self.apply_patch(patch)?;
        }
        MergeType::Reset
      }
    };

    git!(self, "add .")?;
    self.commit("Starter code")?;

    Ok(merge_type)
  }

  fn first_commit_off_main(&self, git_ref: &Ref) -> Result<Ref> {
    let merge_base =
      git_output!(self, "merge-base main {git_ref}").context("Failed to get merge-base")?;
    let merge_base = merge_base.trim();
    let commits = git_output!(self, "rev-list --ancestry-path {merge_base}..{git_ref}")
      .context("Failed to get rev-list")?;
    Ok(Ref::new(commits.lines().last().unwrap()))
  }

  fn head_detached(&self) -> Result<bool> {
    let status = self.git_command("symbolic-ref --quiet HEAD").status()?;
    match status.code() {
      Some(0) => Ok(false),
      Some(1) => Ok(true),
      _ => bail!("symbolic-ref failed"),
    }
  }

  fn current_branch(&self) -> Result<Branch> {
    ensure!(!self.head_detached()?, "head detached");
    let output = git_output!(self, "rev-parse --abbrev-ref HEAD")?;
    Ok(Branch::new(output.trim()))
  }

  /// Given a range of commits described by refs `from..to`, attempts to cherry-pick them onto
  /// the current branch. If this fails, then hard reset to the `to` ref without overwriting the history.
  pub fn cherry_pick_with_fallback(&self, from: &str, to: &str) -> Result<MergeType> {
    let res = git!(self, "cherry-pick {from}..{to}");

    match res {
      Ok(()) => Ok(MergeType::Success),
      Err(e) => {
        tracing::warn!("Merge conflicts when cherry-picking, resorting to hard reset: ${e:?}");

        git!(self, "cherry-pick --abort").context("Failed to abort cherry-pick")?;

        let cur_branch = self.current_branch()?;
        let cur_ref = Ref::from(&cur_branch);
        let first_commit = self.first_commit_off_main(&cur_ref)?;

        git!(self, "reset --hard {to}")?;
        git!(self, "reset --soft origin/{cur_branch}")?;
        git!(self, "checkout {first_commit} README.md")?;
        self.commit("Hard reset to reference solution")?;

        Ok(MergeType::Reset)
      }
    }
  }

  /// Creates a new branch from main, pulling to ensure it's up-to-date.
  ///
  /// TODO: what if there's unstaged changes?
  pub fn create_branch_from_main(&self, branch: &Branch) -> Result<()> {
    self.checkout(&Branch::main())?;
    git!(self, "pull").context("Failed to pull main")?;
    git!(self, "checkout -b {branch}")
  }

  pub fn delete_local_branch(&self, branch: &Branch) -> Result<()> {
    git!(self, "branch -D {branch}")
  }

  pub fn delete_remote_branch(&self, branch: &Branch) -> Result<()> {
    git!(self, "push origin :{branch}")
  }

  /// Runs `git add <file>`
  pub fn add(&self, file: &Path) -> Result<()> {
    git!(self, "add {}", file.display())
  }

  /// Runs `git commit --message=<message>`
  pub fn commit(&self, message: &str) -> Result<()> {
    git!(
      self,
      "commit --message={}",
      shell_escape::escape(message.into())
    )
  }

  /// Runs `git push -u origin <branch>`
  pub fn push(&self, branch: &Branch) -> Result<()> {
    git!(self, "push --set-upstream origin {branch}")
  }

  /// Returns the commit at the head of the current branch
  pub fn head_commit(&self) -> Result<Ref> {
    let output = git_output!(self, "rev-parse HEAD").context("Failed to get head commit")?;
    Ok(Ref::new(output.trim_end()))
  }

  /// Hard resets current branch to `branch` and force pushes current branch.
  pub fn hard_reset(&self, branch: &Branch) -> Result<()> {
    git!(self, "reset --hard {branch}").context("Failed to reset")?;
    git!(self, "push --force --set-upstream origin").context("Failed to push reset branch")
  }

  /// Returns the output of `git diff <base>..<head>`
  pub fn diff(&self, base: &Ref, head: &Ref) -> Result<String> {
    git_output!(self, "diff {base}..{head}")
  }

  /// Returns true if the `branch` head contains a file at `path`
  pub fn contains_file(&self, branch: &Branch, path: &Path) -> Result<bool> {
    let output = command(
      &format!("git cat-file -e {branch}:{}", path.display()),
      &self.path,
    )
    .output()
    .with_context(|| format!("Failed to `git cat-file -e {branch}:{}`", path.display()))?;
    Ok(output.status.success())
  }

  /// Returns the contents of `file` at the head of `branch` as a string
  pub fn read_file_string(&self, branch: &Branch, file: &Path) -> Result<String> {
    git_output!(self, "cat-file -p {branch}:{}", file.display())
  }

  /// Returns the contents of `file` at the head of `branch` as a byte array
  pub fn read_file_bytes(&self, branch: &Branch, file: &Path) -> Result<Vec<u8>> {
    let output = command(
      &format!("git cat-file -p {branch}:{}", file.display()),
      &self.path,
    )
    .output()
    .with_context(|| format!("Failed to `git cat-file -p {branch}:{}", file.display()))?;
    ensure!(
      output.status.success(),
      "git show failed with stderr:\n{}",
      String::from_utf8(output.stderr)?
    );
    Ok(output.stdout)
  }

  /// Returns a list of all file paths checked in at the head of `branch`
  pub fn file_paths(&self, branch: &Branch) -> Result<Vec<PathBuf>> {
    let ls_tree_out = git_output!(self, "ls-tree -r --name-only {branch}")?;
    let paths = ls_tree_out.trim().lines();
    Ok(paths.map(PathBuf::from).collect())
  }

  /// Returns the contents of all files checked in at the head of `branch`
  pub fn read_files(&self, branch: &Branch) -> Result<HashMap<PathBuf, String>> {
    self
      .file_paths(branch)?
      .into_iter()
      .map(|path| {
        let contents = self.read_file_string(branch, &path)?;
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

    self.add(Path::new("."))?;
    self.commit("Initial commit")?;
    git!(self, "tag {INITIAL_TAG}")?;
    self.push(&Branch::main())?;

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

    self.add(Path::new("."))?;
    self.commit("Add meta")?;
    self.push(&Branch::meta())?;
    self.checkout(&Branch::main())?;

    Ok(())
  }

  pub fn checkout(&self, branch: &Branch) -> Result<()> {
    git!(self, "checkout {branch}")
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

  pub fn contains_unstaged_changes(&self) -> Result<bool> {
    let status = self.git_command("diff-index --quiet HEAD --").status()?;
    Ok(!status.success())
  }
}
