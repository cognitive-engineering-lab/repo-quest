pub struct Stage {
  number: usize,
  name: String,
}

impl Stage {
  pub fn new(number: usize, name: impl Into<String>) -> Self {
    Stage {
      number,
      name: name.into(),
    }
  }

  pub fn issue_label(&self) -> String {
    format!("{:02}-{}", self.number, self.name)
  }

  pub fn feature_pr(&self) -> String {
    format!("{:02}a-{}", self.number, self.name)
  }

  pub fn test_pr(&self) -> String {
    format!("{:02}b-{}", self.number, self.name)
  }

  pub fn solution_pr(&self) -> String {
    format!("{:02}c-{}", self.number, self.name)
  }
}
