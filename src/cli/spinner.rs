use std::{io::stdout, time::Duration};

use crossterm::{cursor, execute, style, terminal};
use tokio::{
  select,
  sync::oneshot::{self, Receiver, Sender},
  task::JoinHandle,
  time,
};

pub async fn spinner<F: Future>(msg: impl Into<String>, f: F) -> F::Output {
  let spinner = Spinner::new(msg.into());
  let output = f.await;
  spinner.stop().await;
  output
}

struct Spinner {
  handle: JoinHandle<()>,
  tx: Sender<()>,
}

impl Spinner {
  pub fn new(msg: String) -> Self {
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(Self::spinner_thread(rx, msg));
    Self { handle, tx }
  }

  async fn spinner_thread(mut rx: Receiver<()>, msg: String) {
    let mut frame = 0;
    let mut stdout = stdout();

    loop {
      select! {
        _ = &mut rx => {
          execute!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(terminal::ClearType::CurrentLine),
          ).unwrap();
          return;
        }
        _ = time::sleep(Duration::from_millis(100)) => {
          execute!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(terminal::ClearType::CurrentLine),
            style::Print(FRAMES[frame]),
            style::Print(format!(" {msg}"))
          ).unwrap();
          frame = (frame + 1) % FRAMES.len();
        }
      }
    }
  }

  pub async fn stop(self) {
    self.tx.send(()).unwrap();
    self.handle.await.unwrap()
  }
}

const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
