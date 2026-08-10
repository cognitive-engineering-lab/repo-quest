use std::sync::Arc;

use anyhow::anyhow;
use async_lock::Mutex;
use axum::{Json, extract::State};
use log::debug;
use serde::Deserialize;

use crate::bot::{AppState, Result, set_current_chapter};

#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryHookData {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequestHookData {
    pub number: i64,
    pub merged: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoHookBody {
    pub action: String,
    pub pull_request: Option<PullRequestHookData>,
    pub repository: Option<RepositoryHookData>,
}

pub async fn handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(body): Json<ForgejoHookBody>,
) -> Result<()> {
    debug!("Hook call:\n\n{body:?}\n");
    if let Some(pr) = body.pull_request
        && body.action == "closed"
        && pr.merged
    {
        let mut state = state.lock().await;
        let quest_id = body
            .repository
            .ok_or(anyhow!("No repository info provided with hook body."))?
            .id;

        let quest = state.quest_instances.quest(quest_id)?;
        let corresponding_pr = match quest.metadata.current_task() {
            Some(cur_task) => cur_task.pr.number == pr.number,
            None => pr.number == 0,
        };
        if corresponding_pr {
            let next_chapter_number = quest.metadata.next_chapter();
            if next_chapter_number < quest.metadata.chapter_count() {
                quest.repo.fetch("origin")?;
                quest.repo.hard_reset("refs/remotes/origin/main")?;
                set_current_chapter(&mut state, quest_id, next_chapter_number).await?;
            }
        }
    }
    Ok(())
}
