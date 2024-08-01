#![allow(dead_code)]

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StageConfig {
  pub label: String,
  pub name: String,
  no_starter: Option<bool>,
}

impl StageConfig {
  pub fn no_starter(&self) -> bool {
    self.no_starter.unwrap_or(false)
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
  pub idx: usize,
  pub config: StageConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StagePart {
  Starter,
  Solution,
}

impl StagePart {
  pub fn next_part(self) -> Option<StagePart> {
    match self {
      StagePart::Starter => Some(StagePart::Solution),
      StagePart::Solution => None,
    }
  }

  pub fn parse(s: &str) -> Option<StagePart> {
    match s {
      "a" => Some(StagePart::Starter),
      "b" => Some(StagePart::Solution),
      _ => None,
    }
  }
}

impl fmt::Display for StagePart {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "{}",
      match self {
        StagePart::Starter => "a",
        StagePart::Solution => "b",
      }
    )
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StagePartStatus {
  Start,
  Ongoing,
}

impl StagePartStatus {
  pub fn is_start(self) -> bool {
    matches!(self, StagePartStatus::Start)
  }

  pub fn is_ongoing(self) -> bool {
    matches!(self, StagePartStatus::Ongoing)
  }
}

impl Stage {
  pub fn new(idx: usize, config: StageConfig) -> Self {
    Stage { idx, config }
  }

  pub fn branch_name(&self, part: StagePart) -> String {
    format!("{}-{}", self.config.label, part)
  }
}
