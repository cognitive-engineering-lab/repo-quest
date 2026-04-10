use std::fmt::Display;

pub struct GitTodoList(Vec<String>);

impl Display for GitTodoList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut items = self.0.iter();
        if let Some(item) = items.next() {
            write!(f, "{item}")?;
        }
        for item in items {
            write!(f, "\n{item}")?;
        }

        Ok(())
    }
}

impl GitTodoList {
    #[must_use]
    pub fn new() -> GitTodoList {
        GitTodoList(Vec::new())
    }

    pub fn fixup(&mut self, rev: &str, msg: Option<&str>) {
        let msg = msg.map(|msg| format!(" # {msg}"));
        let msg = msg.as_deref().unwrap_or("");
        self.0.push(format!("fixup {rev}{msg}"));
    }

    pub fn pick(&mut self, rev: &str, msg: Option<&str>) {
        let msg = msg.map(|msg| format!(" # {msg}"));
        let msg = msg.as_deref().unwrap_or("");
        self.0.push(format!("pick {rev}{msg}"));
    }

    pub fn update_branch(&mut self, branch: &str) {
        self.0.push(format!("update-ref refs/heads/{branch}"));
    }
}

impl Default for GitTodoList {
    fn default() -> Self {
        Self::new()
    }
}
