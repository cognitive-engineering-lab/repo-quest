#![allow(dead_code)]

use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stage {
  number: usize,
  pub name: String,
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
  pub fn new(number: usize, name: impl Into<String>) -> Self {
    Stage {
      number,
      name: name.into(),
    }
  }

  pub fn idx(&self) -> usize {
    self.number - 1
  }

  pub fn issue_label(&self) -> String {
    format!("{:02}-{}", self.number, self.name)
  }

  pub fn branch_name(&self, part: StagePart) -> String {
    match part {
      StagePart::Feature => format!("{:02}a-{}", self.number, self.name),
      StagePart::Test => format!("{:02}b-{}", self.number, self.name),
      StagePart::Solution => format!("{:02}c-{}", self.number, self.name),
    }
  }

  pub fn parse(name: &str) -> Option<(Stage, StagePart)> {
    let re = Regex::new(r"^(\d)+(\w)-([\w\d-]+)$").unwrap();
    let cap = re.captures(name)?;
    let (_, [number, part, name]) = cap.extract();
    let number = number.parse::<usize>().ok()?;
    let part = match part {
      "a" => StagePart::Feature,
      "b" => StagePart::Test,
      "c" => StagePart::Solution,
      _ => return None,
    };
    Some((Stage::new(number, name), part))
  }
}
