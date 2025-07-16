use std::fmt;

use serde::{Deserialize, Serialize};

use crate::git::Branch;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Chapter {
  pub label: String,
  pub name: String,
  pub no_starter: Option<bool>,
}

impl Chapter {
  pub fn no_starter(&self) -> bool {
    self.no_starter.unwrap_or(false)
  }

  pub fn branch(&self, part: ChapterPart) -> Branch {
    Branch::new(format!("{}-{}", self.label, part))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChapterPart {
  Starter,
  Solution,
}

impl ChapterPart {
  pub fn next_part(self) -> Option<ChapterPart> {
    match self {
      ChapterPart::Starter => Some(ChapterPart::Solution),
      ChapterPart::Solution => None,
    }
  }

  pub fn parse(s: &str) -> Option<ChapterPart> {
    match s {
      "a" => Some(ChapterPart::Starter),
      "b" => Some(ChapterPart::Solution),
      _ => None,
    }
  }
}

impl fmt::Display for ChapterPart {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ChapterPart::Starter => write!(f, "a"),
      ChapterPart::Solution => write!(f, "b"),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChapterPartStatus {
  Start,
  Ongoing,
}

impl ChapterPartStatus {
  pub fn is_start(self) -> bool {
    matches!(self, ChapterPartStatus::Start)
  }

  pub fn is_ongoing(self) -> bool {
    matches!(self, ChapterPartStatus::Ongoing)
  }
}
