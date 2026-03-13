use std::{
    collections::HashMap,
    convert::Infallible,
    io::{Seek, Write as _},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, anyhow};
use async_lock::Mutex;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Request, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use flate2::read::GzDecoder;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use tar::Archive;
use thiserror::Error;
use tower_http::{cors, normalize_path::NormalizePathLayer};
use tower_layer::Layer as _;
use url::Url;

use crate::{
    forgejo::ForgejoBackend,
    git::GitRepo,
    quest::{definition::*, instance::*},
};

pub const BOT_AUTHOR: Option<&str> = Some("RepoQuest <>");

/// The overall state of ReqoQuest. All of the state is loaded into memory at
/// program startup. Unless a user has many quest definitions or very many quest
/// instances, having everything in memory shouldn't be an issue.
///
/// All changes to the state should be written to disk on each change before the
/// global lock (see the note on concurrency in `main`) is released.
#[derive(Clone)]
pub struct AppState {
    pub forgejo: ForgejoBackend,
    pub forgejo_url: Url,
    pub quest_definitions: QuestDefinitionIndex,
    pub quest_instances: QuestInstanceIndex,
}

#[derive(Debug, Error)]
#[error("Internal server error")]
pub struct AppError(
    #[source]
    #[from]
    pub anyhow::Error,
);

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("{}\n{:?}", self.0, self.0.source()),
        )
            .into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

pub fn new(
    state: AppState,
) -> impl tower_service::Service<
    Request,
    Response = axum::http::Response<axum::body::Body>,
    Error = Infallible,
    Future: Send,
> + Clone {
    let cors = cors::CorsLayer::new()
        .allow_origin(cors::Any)
        .allow_headers(cors::Any);

    // build our application with a route
    let app = Router::new()
        // POST: Endpoint for Forgejo webhooks.
        .route("/hook", post(crate::forgejo_hook::handler))
        // GET: A map from available quest template ids to names and to
        // descriptions
        //
        // POST: Creates a new quest template from source.
        .route(
            "/quest_definition",
            get(get_quest_definitions)
                // allow quest definitions to be large
                .post(add_quest_definition)
                .layer(DefaultBodyLimit::disable()),
        )
        // GET: Gets the list of current quest ids and the names of the
        // templates the quests are based on.
        //
        // POST: Creates a quest and returns the ID of the quest on success.
        .route("/quest", get(get_quests).post(start_quest))
        // GET: Gets the list of chapters for a quest and the corresponding
        // issue IDs and PR IDs, if there are any. The chapter ID is the index
        // in the list and if there are no issue IDs or PRs, the chapter hasn't
        // been started yet.
        .route("/quest/{questId}/chapter", get(get_chapters))
        // GET: Gets the current chapter of the given quest along with the
        // corresponding issue ID and PR ID.
        //
        // POST: Sets the quest chapter to the requested chapter, if the
        // requested chapter is next and the current chapter is finished.
        .route(
            "/quest/{questId}/chapter/current",
            get(get_current_chapter).post(post_set_current_chapter),
        )
        // GET: Gets the PR ID for the reference solution for the given chapter
        // if there is one, or whether there is one available (if there isn't).
        //
        // POST: Creates the reference solution for the given chapter and
        // returns the PR ID, if the given chapter is current.
        .route(
            "/quest/{questId}/chapter/{chapterId}/reference_solution",
            get(get_reference_solution).post(create_reference_solution),
        )
        .route(
            "/quest/{questId}/chapter/current/reference_solution",
            get(get_current_reference_solution).post(create_current_reference_solution),
        )
        // A note on concurrency:
        //
        // At the moment requests are completely serialized by using async_lock
        // so that one lock can be taken at the beginning of every request and
        // held across calls to the Forgejo instance. This should suffice
        // because we anticipate the instance to be used by a single user, and
        // so concurrent requests should be rare and (because the Forgejo
        // instance is local to the reqpo-quest process) should be resolved
        // quickly enough that there is little benefit to handling them in
        // parallel.
        .with_state(Arc::new(Mutex::new(state)))
        .layer(cors);
    NormalizePathLayer::trim_trailing_slash().layer(app)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestDefinitionInfo {
    id: usize,
    name: String,
    description: String,
}

async fn get_quest_definitions(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<Json<Vec<QuestDefinitionInfo>>> {
    let state = state.lock().await;

    let quest_definitions = &state.quest_definitions;
    let mut result = Vec::with_capacity(quest_definitions.len());
    for quest_id in 0..quest_definitions.len() {
        let quest_definition = quest_definitions.definition(quest_id)?;
        result.push(QuestDefinitionInfo {
            id: quest_id,
            name: quest_definition.metadata.title.clone(),
            description: quest_definition.metadata.description.clone(),
        });
    }
    Ok(Json(result))
}

async fn add_quest_definition(
    State(state): State<Arc<Mutex<AppState>>>,
    mut bundle: Multipart,
) -> Result<Json<QuestDefinitionInfo>> {
    let mut state = state.lock().await;
    // save tarball
    let mut tar_gz = tempfile::tempfile().context("Could not create tempfile for downloading.")?;
    {
        let mut found = false;
        // drop file content early, in case it is big
        while let Some(content) = bundle.next_field().await.context("File upload failed.")? {
            debug!("multipart field: {:?}", content.name());
            if content.name().is_some_and(|n| n == "quest-bundle") {
                // TODO: write content as it streams in.
                let bytes = content
                    .bytes()
                    .await
                    .context("Could not get uploaded file data.")?;
                tar_gz
                    .write_all(&bytes)
                    .with_context(|| format!("Could not write bundle to disk at {tar_gz:?}."))?;
                // TODO: is this needed if we're using the same file handle?
                tar_gz.flush().with_context(|| {
                    format!("Could not flush after writing bundle to disk at {tar_gz:?}.")
                })?;
                tar_gz
                    .seek(std::io::SeekFrom::Start(0))
                    .context("Could not seek to start of bundle file on disk.")?;

                found = true;
                break;
            }
        }
        if !found {
            return Err(AppError(anyhow!("No file uploaded.")));
        }
    }

    let dir = tempfile::Builder::new()
        .disable_cleanup(true)
        .prefix("quest-defn")
        .tempdir_in(&state.quest_definitions.dir)
        .context("Could not create directory to unpack quest bundle.")?;
    let mut archive = Archive::new(GzDecoder::new(tar_gz));
    archive
        .unpack(&dir)
        .context("Could not unpack quest bundle.")?;

    // TODO: validate bundle
    // - check that metadata file parses
    // - check that repo is a git repo with a meta branch and file, etc.

    // add to index
    let quest_defn_id = state.quest_definitions.insert(dir.path().to_path_buf());
    state.quest_definitions.store()?;

    let metadata = state.quest_definitions.metadata(quest_defn_id)?;
    Ok(Json(QuestDefinitionInfo {
        id: quest_defn_id,
        name: metadata.title,
        description: metadata.description,
    }))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestInfo {
    name: String,
    owner: String,
    repo: String,
    task_info: Option<Task>,
}

// TODO: switch to list for consistent ordering
async fn get_quests(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<Json<HashMap<i64, QuestInfo>>> {
    let state = state.lock().await;
    let quests = &state.quest_instances;
    let quest_defns = &state.quest_definitions;
    let mut quests_info = HashMap::new();
    for id in quests.keys() {
        let quest = quests.metadata(id)?;
        let quest_defn = quest_defns.definition(quest.definition_id)?;
        let quest_info = QuestInfo {
            name: quest_defn.metadata.title.clone(),
            owner: quest.owner.clone(),
            repo: quest.repo.clone(),
            task_info: quest.tasks.last().cloned(),
        };
        quests_info.insert(id, quest_info);
    }
    Ok(Json(quests_info))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartQuestQuery {
    username: String,
    quest_template_id: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartQuestResponse {
    /// RepoQuest ID for the quest
    id: i64,
    /// Location of the just-created repo
    repo_url: Url,
    /// First task information. `None` if the quest has no tasks.
    task: Option<Task>,
}

async fn start_quest(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(query): Json<StartQuestQuery>,
) -> Result<Json<StartQuestResponse>> {
    let mut state = state.lock().await;

    let QuestDefinition {
        metadata: template, ..
    } = state
        .quest_definitions
        .definition(query.quest_template_id)?
        .clone();

    // generate forgejo repo from template
    let forgejo_repo = state
        .forgejo
        .create_quest_repo(&query.username, &template)
        .await?;
    let id = forgejo_repo.id.context("No repo id.")?;
    let repo_url = forgejo_repo.html_url.context("Forgejo repo has no url.")?;

    // define quest instance
    let quest = QuestMetadata {
        definition_id: query.quest_template_id,
        owner: query.username,
        repo: forgejo_repo.name.context("No repo name")?,
        tasks: Vec::new(),
    };

    // write quest to file and add to index
    debug!("Storing quest metadata.");
    state.quest_instances.insert_quest(id, &quest)?;

    // save index
    state.quest_instances.store()?;

    // create local quest repo
    let local_repo = GitRepo::init(state.quest_instances.repo_path(id)?)?;
    debug!("Initialized local git repo {local_repo:?}");

    // set the quest definition repo as a remote
    let defn_repo_path = state.quest_definitions.repo_path(query.quest_template_id)?;
    local_repo.add_remote(
        "quest",
        defn_repo_path.to_str().with_context(|| {
            format!("Could not convert template repo path {defn_repo_path:?} to string.")
        })?,
    )?;

    // fetch and initialize
    local_repo.fetch("quest")?;
    local_repo.hard_reset("quest/main")?;

    // set repo upstream to forgejo
    let mut remote_url = repo_url.clone();
    remote_url
        .set_username("repoquest")
        .map_err(|_| anyhow!("Can't set remote username."))?;
    remote_url
        .set_password(Some("repoquest"))
        .map_err(|_| anyhow!("Can't set remote password"))?;
    remote_url
        .set_host(state.forgejo_url.host_str())
        .map_err(|_| anyhow!("Can't set remote host."))?;
    remote_url
        .set_port(state.forgejo_url.port())
        .map_err(|_| anyhow!("Can't set remote port."))?;

    local_repo.add_remote("origin", remote_url.as_str())?;

    // push starter code to upstream main
    local_repo.push("origin", "main", "main")?;

    // start first chapter, if there are chapters
    let task = if !template.tasks.is_empty() {
        // Forgejo can't accept PRs right away... this works around that.
        // TODO: don't return from repo creation until the repo is fully created.
        std::thread::sleep(Duration::from_secs(2));
        Some(set_current_chapter(&mut state, id, 0).await?)
    } else {
        None
    };

    Ok(Json(StartQuestResponse { id, repo_url, task }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSolutionQuery {
    quest_id: i64,
    chapter_id: Option<usize>,
}

async fn create_current_reference_solution(
    state: State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<i64>,
) -> Result<Json<PullRequest>> {
    create_reference_solution(
        state,
        Path(ReferenceSolutionQuery {
            quest_id,
            chapter_id: None,
        }),
    )
    .await
}

async fn create_reference_solution(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(query): Path<ReferenceSolutionQuery>,
) -> Result<Json<PullRequest>> {
    let mut state = state.lock().await;
    let ReferenceSolutionQuery {
        quest_id,
        chapter_id,
    } = query;

    let AppState {
        ref forgejo,
        quest_instances: ref mut quests,
        quest_definitions: ref defns,
        ..
    } = *state;

    let Quest {
        metadata: ref mut quest,
        repo: local_repo,
        ..
    } = quests.quest(quest_id)?;

    let quest_definition_id = quest.definition_id;

    let quest_definition = defns.definition(quest_definition_id)?;

    let current_chapter = quest.tasks.len().checked_sub(1);
    let chapter_id = chapter_id
        .or(current_chapter)
        .context("No chapter for which to get reference solution.")?;

    let requested_task_instance = quest
        .tasks
        .get_mut(chapter_id)
        .with_context(|| format!("Quest instance {quest_id} has no chapter {chapter_id}."))?;

    let pr = if let Some(pr) = &requested_task_instance.reference_solution {
        pr.clone()
    } else {
        let requested_task = quest_definition
        .metadata
        .tasks
        .get(chapter_id)
        .with_context(|| {
            format!(
                "Quest {quest_id} for definition {quest_definition_id} has no chapter {chapter_id}."
            )
        })?;

        // TODO: Figure out how to open the PR for various circumstances, such as
        // for a previously-completed chapter where the scaffolding branch has been
        // deleted.
        let remote_solution_branch = format!(
            "refs/remotes/quest/{}",
            &requested_task.reference_solution.0
        );
        let local_solution_branch = &requested_task.reference_solution.0;
        let remote_scaffold_branch =
            format!("refs/remotes/quest/{}", &requested_task.scaffolding.0);
        let local_scaffold_branch = &requested_task.scaffolding.0;

        let initial_scaffold_hash = &requested_task_instance.initial_scaffolding_hash;

        local_repo.create_branch(&remote_solution_branch, local_solution_branch)?;
        local_repo.switch_branch(local_solution_branch)?;
        local_repo.rebase(initial_scaffold_hash, &remote_scaffold_branch)?;
        local_repo.push("origin", local_solution_branch, local_solution_branch)?;
        local_repo.switch_branch("main")?;

        let pr_title = format!(
            "Reference solution for {}",
            &requested_task.issue_template.title,
        );
        let pr = forgejo
            .create_pr(
                quest.owner.clone(),
                quest.repo.clone(),
                local_scaffold_branch.to_string(),
                local_solution_branch.to_string(),
                pr_title,
                "".to_string(),
            )
            .await?;

        requested_task_instance.reference_solution = Some(pr.clone());
        quests.store_quest(quest_id, quest)?;

        pr
    };

    Ok(Json(pr))
}

async fn get_current_reference_solution(
    state: State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<i64>,
) -> Result<Json<Option<PullRequest>>> {
    get_reference_solution(
        state,
        Path(ReferenceSolutionQuery {
            quest_id,
            chapter_id: None,
        }),
    )
    .await
}

async fn get_reference_solution(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(query): Path<ReferenceSolutionQuery>,
) -> Result<Json<Option<PullRequest>>> {
    let state = state.lock().await;
    let ReferenceSolutionQuery {
        quest_id,
        chapter_id,
    } = query;

    let quest = state.quest_instances.metadata(quest_id)?;
    let chapter_id = chapter_id.unwrap_or(
        quest
            .tasks
            .len()
            .checked_sub(1)
            .with_context(|| format!("Quest instance {quest_id} has no chapters."))?,
    );

    let task = quest
        .tasks
        .get(chapter_id)
        .with_context(|| format!("Quest instance {quest_id} has no chapter {chapter_id}."))?;

    Ok(Json(task.reference_solution.clone()))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NextChapterBody {
    chapter_number: usize,
}

async fn get_chapters(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<i64>,
) -> Result<Json<Vec<Option<Task>>>> {
    let state = state.lock().await;

    let quests = &state.quest_instances;

    let quest = quests
        .metadata(quest_id)
        .with_context(|| format!("No quest with id {quest_id}."))?;

    let mut tasks = quest
        .tasks
        .iter()
        .cloned()
        .map(Some)
        .collect::<Vec<Option<Task>>>();

    let quest_definitions = &state.quest_definitions;
    let task_templates_len = quest_definitions
        .definition(quest.definition_id)
        .with_context(|| {
            format!(
                "No quest template id {}, for quest {quest_id}.",
                &quest.definition_id
            )
        })?
        .metadata
        .tasks
        .len();

    tasks.extend((1..task_templates_len - tasks.len()).map(|_| None));

    Ok(Json(tasks))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChapterInfo {
    id: usize,
    task_name: String,
    task: Task,
}

async fn get_current_chapter(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<i64>,
) -> Result<Json<Option<ChapterInfo>>> {
    let state = state.lock().await;
    let quests = &state.quest_instances;

    let quest = quests
        .metadata(quest_id)
        .with_context(|| format!("No quest with id {quest_id}."))?;
    let quest_definition = state
        .quest_definitions
        .definition(quest.definition_id)
        .with_context(|| format!("No quest definition with id {}.", quest.definition_id))?;

    let response = if let Some(cur_task_id) = quest.tasks.len().checked_sub(1) {
        let cur_task = &quest.tasks[cur_task_id];
        let cur_task_definition = &quest_definition.metadata.tasks[cur_task_id];
        let task_name = cur_task_definition.issue_template.title.clone();
        Some(ChapterInfo {
            id: cur_task_id,
            task_name,
            task: cur_task.clone(),
        })
    } else {
        None
    };
    Ok(Json(response))
}

/// Sets the current chapter to the requested chapter, if the requested chapter
/// is the next chapter.
async fn post_set_current_chapter(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<i64>,
    Json(body): Json<NextChapterBody>,
) -> Result<Json<Task>> {
    info!("Set current chapter for {quest_id} to {body:?}.");

    let mut state = state.lock().await;
    Ok(Json(
        set_current_chapter(&mut state, quest_id, body.chapter_number).await?,
    ))
}

pub async fn set_current_chapter(
    state: &mut AppState,
    quest_id: i64,
    chapter_number: usize,
) -> Result<Task> {
    let AppState {
        ref forgejo,
        quest_instances: ref mut quests,
        quest_definitions: ref defns,
        ..
    } = *state;

    let Quest {
        metadata: quest,
        repo: local_repo,
        ..
    } = quests
        .quest(quest_id)
        .with_context(|| format!("No quest with id {quest_id}."))?;

    let quest_definition = defns.definition(quest.definition_id)?;

    let next_task_pos = quest.tasks.len();

    if next_task_pos != chapter_number {
        return Err(AppError(anyhow!(
            "Requested chapter {chapter_number} is not the next chapter."
        )));
    }

    // Confirm that previous task is complete.
    if let Some(task) = quest.tasks.last()
        && !forgejo
            .is_pull_request_merged(&quest.owner, &quest.repo, task.pr.number)
            .await?
    {
        return Err(anyhow!(
            "Pull request {}/{}#{} not merged.",
            &quest.owner,
            &quest.repo,
            task.pr.number
        )
        .into());
    }

    let task_template = quest_definition
        .metadata
        .tasks
        .get(chapter_number)
        .with_context(|| format!("Missing definition of task for chapter {chapter_number}."))?;
    let prev_task_solution_branch = match chapter_number.checked_sub(1) {
        None => "main",
        Some(prev_chapter_number) => {
            &quest_definition
                .metadata
                .tasks
                .get(prev_chapter_number)
                .with_context(|| {
                    format!("Missing definition of task for chapter {chapter_number}.")
                })?
                .reference_solution
                .0
        }
    };

    let scaffolding = &task_template.scaffolding.0;
    local_repo.create_branch(&format!("refs/remotes/quest/{scaffolding}"), scaffolding)?;
    local_repo.switch_branch(scaffolding)?;
    let remote_prev_solution_branch = format!("refs/remotes/quest/{prev_task_solution_branch}");
    // TODO: return list of original/new commit hashes for later use in aligning PR comments
    let hashes = match local_repo.rebase("main", &remote_prev_solution_branch) {
        Err(_) => {
            // If the rebase fails, first reset to the previous reference solution branch and then
            // rebase onto that.
            local_repo.restore_from(&remote_prev_solution_branch)?;
            local_repo.commit("Reset to the reference solution", BOT_AUTHOR)?;
            local_repo.rebase("main", &remote_prev_solution_branch)?
        }
        Ok(res) => res,
    };
    local_repo.push("origin", scaffolding, scaffolding)?;
    local_repo.switch_branch("main")?;

    let mut task_info = HashMap::new();
    for (task_id, chapter_num) in quest_definition.metadata.task_ids {
        if let Some(task) = quest.tasks.get(chapter_num) {
            task_info.insert(format!("{} pr", task_id), format!("#{}", task.pr.number));
            task_info.insert(
                format!("{} issue", task_id),
                format!("#{}", task.issue.number),
            );
        }
    }

    let initial_scaffolding_hash = local_repo.rev_parse(scaffolding)?;

    let task = forgejo
        .create_task(
            quest.owner.clone(),
            quest.repo.clone(),
            task_template,
            task_info,
            hashes,
            initial_scaffolding_hash,
        )
        .await?;

    let mut quest = quests
        .metadata(quest_id)
        .with_context(|| format!("No quest with id {quest_id}."))?;

    quest.tasks.push(task.clone());

    quests.store_quest(quest_id, &quest)?;

    Ok(task)
}
