mod forgejo_hook;

use std::{
    collections::HashMap,
    env, fs,
    io::{Seek, Write as _},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, anyhow};
use async_lock::Mutex;
use axum::{
    Json, Router, ServiceExt,
    extract::{DefaultBodyLimit, Multipart, Path, Request, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use env_logger::Env;
use flate2::read::GzDecoder;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use tar::Archive;
use thiserror::Error;
use tower_http::{cors, normalize_path::NormalizePathLayer};
use tower_layer::Layer as _;
use url::Url;

use repo_quest::{
    forgejo::ForgejoBackend,
    git::GitRepo,
    quest::{definition::*, instance::*},
};

/// The overall state of ReqoQuest. All of the state is loaded into memory at
/// program startup. Unless a user has many quest definitions or very many quest
/// instances, having everything in memory shouldn't be an issue.
///
/// All changes to the state should be written to disk on each change before the
/// global lock (see the note on concurrency in `main`) is released.
#[derive(Clone)]
struct AppState {
    forgejo: ForgejoBackend,
    quest_definitions: QuestDefinitionIndex,
    quest_instances: QuestInstanceIndex,
}

#[derive(Debug, Error)]
#[error("Internal server error")]
struct AppError(
    #[source]
    #[from]
    anyhow::Error,
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

type Result<T> = std::result::Result<T, AppError>;

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(not(debug_assertions))]
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

    let args: Vec<String> = env::args().collect();

    let given_quest_dir = PathBuf::from(args.get(1).unwrap());
    if !given_quest_dir.is_dir() {
        fs::create_dir_all(&given_quest_dir)
            .with_context(|| format!("Could not create dir {given_quest_dir:?}"))?;
    }
    let quest_dir = given_quest_dir
        .canonicalize()
        .with_context(|| format!("Could not canonicalize quest dir path {given_quest_dir:?}"))?;
    info!("Quest directory {quest_dir:?}.");

    let forgejo = {
        let auth = forgejo_api::Auth::Password {
            username: "repoquest",
            password: "repoquest",
            mfa: None,
        };
        let url = Url::parse("http://localhost:3000").unwrap();
        ForgejoBackend::new(auth, url)
    };
    let quest_definitions = QuestDefinitionIndex::load_or_init(quest_dir.join("definitions"))?;
    let quest_instances = QuestInstanceIndex::load_or_init(quest_dir.join("instances"))?;

    // register to receive webhooks
    forgejo
        .register_webhook(Url::parse("http://localhost:8000/hook").unwrap())
        .await?;

    // A note on concurrency:
    //
    // At the moment requests are completely serialized by using async_lock so that
    // one lock can be taken at the beginning of every request and held across calls
    // to the Forgejo instance. This should suffice because we anticipate the
    // instance to be used by a single user, and so concurrent requests should be
    // rare and (because the Forgejo instance is local to the reqpo-quest process)
    // should be resolved quickly enough that there is little benefit to handling
    // them in parallel.
    let state = Arc::new(Mutex::new(AppState {
        forgejo,
        quest_definitions,
        quest_instances,
    }));

    let cors = cors::CorsLayer::new()
        .allow_origin(cors::Any)
        .allow_headers(cors::Any);

    // build our application with a route
    let app = Router::new()
        // POST: Endpoint for Forgejo webhooks.
        .route("/hook", post(forgejo_hook::handler))
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
        .with_state(state)
        .layer(cors);
    let app = NormalizePathLayer::trim_trailing_slash().layer(app);

    // run our app with hyper, listening globally on port 8000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    axum::serve(listener, ServiceExt::<Request>::into_make_service(app))
        .await
        .unwrap();

    Ok(())
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
    repo_url: Url,
    task_info: Option<Task>,
}

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
            repo_url: quest.repo_url.clone(),
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
    /// HTML location for the next step of the quest
    url: Url,
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
        repo_url: repo_url.clone(),
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
    local_repo.restore_from("quest/main")?;
    local_repo.commit("Initial commit")?;

    // set repo upstream to forgejo
    let mut remote_url = repo_url.clone();
    remote_url
        .set_username("repoquest")
        .map_err(|_| anyhow!("Can't set remote username."))?;
    remote_url
        .set_password(Some("repoquest"))
        .map_err(|_| anyhow!("Can't set remote password"))?;

    local_repo.add_remote("origin", remote_url.as_str())?;

    // push starter code to upstream main
    local_repo.push("origin", "main", "main")?;

    let mut url = repo_url;
    // start first chapter, if there are chapters
    if !template.tasks.is_empty() {
        // Forgejo can't accept PRs right away... this works around that.
        // TODO: don't return from repo creation until the repo is fully created.
        std::thread::sleep(Duration::from_secs(2));
        let task = set_current_chapter(&mut state, id, 0).await?;
        url = task.issue.url;
    };

    Ok(Json(StartQuestResponse { id, url }))
}

async fn create_reference_solution(
    State(_state): State<Arc<Mutex<AppState>>>,
    Path(_quest_id): Path<String>,
    Path(_chapter_id): Path<String>,
) -> Result<()> {
    todo!()
}

async fn get_reference_solution(
    State(_state): State<Arc<Mutex<AppState>>>,
    Path(_quest_id): Path<i64>,
    Path(_chapter_id): Path<String>,
) -> Result<Json<i64>> {
    todo!()
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
    issue_url: Url,
    pr_url: Url,
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

    let response = if let Some(cur_task_id) = quest.tasks.len().checked_sub(1) {
        let cur_task = &quest.tasks[cur_task_id];
        Some(ChapterInfo {
            id: cur_task_id,
            issue_url: cur_task.issue.url.clone(),
            pr_url: cur_task.pr.url.clone(),
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

async fn set_current_chapter(
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
            local_repo.commit("Reset to the reference solution")?;
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
            &quest.owner,
            &quest.repo,
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
