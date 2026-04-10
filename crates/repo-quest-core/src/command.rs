use anyhow::{Context as _, Result, anyhow};
use std::{
    fmt::{Debug, Display},
    process::Command,
};

/// Extension trait for running commands to completion with an anyhow context
/// message on a failure exit code.
pub trait RunCommand {
    fn run_with_context<C, F>(&mut self, f: F) -> Result<()>
    where
        C: Display + Debug + Send + Sync + 'static,
        F: FnOnce() -> C;

    fn stdout_with_context<C, F>(&mut self, f: F) -> Result<String>
    where
        C: Display + Debug + Send + Sync + 'static,
        F: FnOnce() -> C;

    fn line_with_context<C, F>(&mut self, f: F) -> Result<String>
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
        let output = self.output()?;
        if output.status.success() {
            log::info!(
                "{:?} stdout: {}",
                self,
                String::from_utf8_lossy(&output.stdout)
            );
            log::info!(
                "{:?} stderr: {}",
                self,
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        } else {
            log::error!(
                "{:?} stdout: {}",
                self,
                String::from_utf8_lossy(&output.stdout)
            );
            log::error!(
                "{:?} stderr: {}",
                self,
                String::from_utf8_lossy(&output.stderr)
            );
            Err(anyhow!(f()))
        }
    }

    fn stdout_with_context<C, F>(&mut self, f: F) -> Result<String>
    where
        C: Display + Debug + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        let output = self.output()?;
        if output.status.success() {
            log::info!(
                "{:?} stdout: {}",
                self,
                String::from_utf8_lossy(&output.stdout)
            );
            log::info!(
                "{:?} stderr: {}",
                self,
                String::from_utf8_lossy(&output.stderr)
            );
            let output_content = String::from_utf8(output.stdout)?;
            Ok(output_content)
        } else {
            log::error!(
                "{:?} stdout: {}",
                self,
                String::from_utf8_lossy(&output.stdout)
            );
            log::error!(
                "{:?} stderr: {}",
                self,
                String::from_utf8_lossy(&output.stderr)
            );
            Err(anyhow!(
                "Child process exited with non-success exit code {}.",
                output.status
            ))
            .with_context(f)
        }
    }

    fn line_with_context<C, F>(&mut self, f: F) -> Result<String>
    where
        C: Display + Debug + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        let output = self.output()?;
        if output.status.success() {
            log::info!(
                "{:?} stdout: {}",
                self,
                String::from_utf8_lossy(&output.stdout)
            );
            log::info!(
                "{:?} stderr: {}",
                self,
                String::from_utf8_lossy(&output.stderr)
            );
            let output_content = String::from_utf8(output.stdout)?;
            Ok(output_content
                .lines()
                .nth(0)
                .context("Command output is empty when at least one line expected.")
                .with_context(f)?
                .to_string())
        } else {
            log::error!(
                "{:?} stdout: {}",
                self,
                String::from_utf8_lossy(&output.stdout)
            );
            log::error!(
                "{:?} stderr: {}",
                self,
                String::from_utf8_lossy(&output.stderr)
            );
            Err(anyhow!(
                "Child process exited with non-success exit code {}.",
                output.status
            ))
            .with_context(f)
        }
    }
}
