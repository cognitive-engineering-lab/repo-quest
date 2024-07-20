#![allow(dead_code)]

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageConfig {
  pub label: String,
  pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
  pub idx: usize,
  pub config: StageConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StagePart {
  Feature,
  Test,
  Solution,
}

impl StagePart {
  pub fn next_part(self) -> Option<StagePart> {
    match self {
      StagePart::Feature => Some(StagePart::Test),
      StagePart::Test => Some(StagePart::Solution),
      StagePart::Solution => None,
    }
  }

  pub fn parse(s: &str) -> Option<StagePart> {
    match s {
      "a" => Some(StagePart::Feature),
      "b" => Some(StagePart::Test),
      "c" => Some(StagePart::Solution),
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
        StagePart::Feature => "a",
        StagePart::Test => "b",
        StagePart::Solution => "c",
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
