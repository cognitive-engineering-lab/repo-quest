use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub struct Stage {
  pub label: String,
  pub name: String,
  pub no_starter: Option<bool>,
}

impl Stage {
  pub fn no_starter(&self) -> bool {
    self.no_starter.unwrap_or(false)
  }

  pub fn branch_name(&self, part: StagePart) -> String {
    format!("{}-{}", self.label, part)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
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
