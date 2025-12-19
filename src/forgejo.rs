use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use crate::quest::{
    definition::{QuestDefinitionMetadata, TaskTemplate},
    instance::{Issue, PullRequest, Task},
};
use anyhow::{Context as _, Result, anyhow};
use forgejo_api::{
    Auth, Forgejo,
    structs::{
        AddCollaboratorOption, AddCollaboratorOptionPermission, CreateHookOptionConfig,
        CreateHookOptionType, CreateIssueCommentOption, CreateIssueOption, CreatePullRequestOption,
        CreatePullReviewComment, CreatePullReviewOptions, CreateRepoOption, EditIssueOption,
        EditPullRequestOption, EditRepoOption, Hook, Repository, UserListReposQuery,
    },
};
use log::debug;
use url::Url;

#[derive(Clone)]
pub struct ForgejoBackend {
    forgejo: Arc<Forgejo>,
}

impl ForgejoBackend {
    pub fn new(auth: Auth, url: Url) -> ForgejoBackend {
        let forgejo = Arc::new(Forgejo::new(auth, url).unwrap());

        ForgejoBackend { forgejo }
    }

    pub async fn is_pull_request_merged(&self, owner: &str, repo: &str, id: u64) -> Result<bool> {
        let pr = self
            .forgejo
            .repo_get_pull_request(owner, repo, id)
            .await
            .with_context(|| format!("Could not get pr {owner}/{repo}#{id}."))?;
        pr.merged.ok_or(anyhow!(
            "Could not get merge status of pr {owner}/{repo}#{id}."
        ))
    }

    async fn all_pages<T, Fut>(&self, f: impl Fn(u32) -> Fut) -> Result<Vec<T>>
    where
        Fut: Future<Output = Result<(i64, Vec<T>)>>,
    {
        let mut page = 0;
        let (total_count, mut res) = f(page).await?;
        let mut current_count = res.len();
        let mut items = res;
        while current_count < total_count.try_into().unwrap_or(0) {
            page += 1;
            (_, res) = f(page).await?;
            current_count += res.len();
            items.append(&mut res);
        }

        Ok(items)
    }

    async fn user_repos_page(&self, username: &str, page: u32) -> Result<(i64, Vec<Repository>)> {
        let res = self
            .forgejo
            .user_list_repos(
                username,
                UserListReposQuery {
                    page: Some(page),
                    ..Default::default()
                },
            )
            .await?;
        Ok((res.0.x_total_count.unwrap_or(0), res.1))
    }

    async fn user_repos(&self, username: &str) -> Result<Vec<Repository>> {
        self.all_pages(|page| self.user_repos_page(username, page))
            .await
    }

    async fn fresh_repo_name(&self, username: &str, basename: String) -> Result<String> {
        let repo_names: Vec<String> = self
            .user_repos(username)
            .await?
            .into_iter()
            .filter_map(|r| r.name) // TODO: what does it mean for a repo to have no name?
            .collect();

        if !repo_names.contains(&basename) {
            Ok(basename)
        } else {
            let mut suffix = 1;
            let mut name = format!("{basename}-{suffix}");
            while repo_names.contains(&name) {
                suffix += 1;
                name = format!("{basename}-{suffix}");
            }

            Ok(name)
        }
    }

    pub async fn create_quest_repo(
        &self,
        username: &str,
        template: &QuestDefinitionMetadata,
    ) -> Result<Repository> {
        let repo_name = self
            .fresh_repo_name(username, template.generated_repo_name.clone())
            .await?;
        let res = self
            .forgejo
            .admin_create_repo(
                username,
                CreateRepoOption {
                    auto_init: Some(false),
                    default_branch: Some("main".to_string()),
                    description: Some(template.title.clone()),
                    gitignores: None,
                    issue_labels: None,
                    license: None,
                    name: repo_name.clone(),
                    object_format_name: None,
                    private: Some(false),
                    readme: None,
                    template: None,
                    trust_model: None,
                },
            )
            .await?;
        self.forgejo
            .repo_add_collaborator(
                username,
                &repo_name,
                "repoquest",
                AddCollaboratorOption {
                    permission: Some(AddCollaboratorOptionPermission::Admin),
                },
            )
            .await?;
        self.forgejo
            .repo_edit(
                username,
                &repo_name,
                EditRepoOption {
                    allow_fast_forward_only_merge: None,
                    allow_manual_merge: None,
                    allow_merge_commits: None,
                    allow_rebase: None,
                    allow_rebase_explicit: None,
                    allow_rebase_update: None,
                    allow_squash_merge: None,
                    archived: None,
                    autodetect_manual_merge: Some(true),
                    default_allow_maintainer_edit: None,
                    default_branch: None,
                    default_delete_branch_after_merge: None,
                    default_merge_style: None,
                    default_update_style: None,
                    description: None,
                    enable_prune: None,
                    external_tracker: None,
                    external_wiki: None,
                    globally_editable_wiki: None,
                    has_actions: None,
                    has_issues: None,
                    has_packages: Some(false),
                    has_projects: Some(false),
                    has_pull_requests: None,
                    has_releases: Some(false),
                    has_wiki: Some(false),
                    ignore_whitespace_conflicts: None,
                    internal_tracker: None,
                    mirror_interval: None,
                    name: None,
                    private: None,
                    template: None,
                    website: None,
                    wiki_branch: None,
                },
            )
            .await?;
        Ok(res)
    }

    pub async fn create_task(
        &self,
        username: &str,
        repo_name: &str,
        template: &TaskTemplate,
        mut task_info: HashMap<String, String>,
        hashes: HashMap<String, String>,
    ) -> Result<Task> {
        let issue = self
            .forgejo
            .issue_create_issue(
                username,
                repo_name,
                CreateIssueOption {
                    assignee: Some(username.to_string()),
                    assignees: None,
                    body: None,
                    closed: None,
                    due_date: None,
                    labels: None,
                    milestone: None,
                    r#ref: None,
                    title: template.issue_template.title.clone(),
                },
            )
            .await
            .with_context(|| "Couldn't create issue.")?;
        let issue_number = issue.number.ok_or(anyhow!("No issue id."))? as u64;
        debug!("Created issue {username}/{repo_name}/{issue_number}.");

        let pr_title = template
            .pr_template
            .clone()
            .map_or_else(|| template.issue_template.title.clone(), |t| t.title);
        let pr = self
            .forgejo
            .repo_create_pull_request(
                username,
                repo_name,
                CreatePullRequestOption {
                    assignee: Some(username.to_string()),
                    assignees: None,
                    base: Some("main".to_string()),
                    body: None,
                    due_date: None,
                    // TODO: synthesize branch name
                    head: Some(template.scaffolding.0.clone()),
                    labels: None,
                    milestone: None,
                    title: Some(pr_title),
                },
            )
            .await
            .with_context(|| "Couldn't create PR.")?;
        let pr_number = pr.number.ok_or(anyhow!("No PR id."))? as u64;
        debug!("Created pull request {username}/{repo_name}/{pr_number}.");

        task_info.insert(format!("{} pr", template.task_id), format!("#{pr_number}"));
        task_info.insert(
            format!("{} issue", template.task_id),
            format!("#{issue_number}"),
        );

        let issue_body = template
            .issue_template
            .body
            .instantiate(&task_info)
            .context("Couldn't instantiate issue template")?;
        self.forgejo
            .issue_edit_issue(
                username,
                repo_name,
                issue_number,
                EditIssueOption {
                    assignee: None,
                    assignees: None,
                    body: Some(issue_body),
                    due_date: None,
                    milestone: None,
                    r#ref: None,
                    state: None,
                    title: None,
                    unset_due_date: None,
                    updated_at: None,
                },
            )
            .await
            .with_context(|| format!("Couldn't edit issue with ID {issue_number}"))?;
        debug!("Updated issue {username}/{repo_name}/{issue_number}.");

        let pr_body = if let Some(pr_template) = template.pr_template.as_ref() {
            pr_template
                .body
                .instantiate(&task_info)
                .context("Couldn't instantiate PR template")?
        } else {
            format!(
                "This PR resolves #{issue_number}. (Don't merge until you've added your solution!)"
            )
        };
        self.forgejo
            .repo_edit_pull_request(
                username,
                repo_name,
                pr_number,
                EditPullRequestOption {
                    assignee: None,
                    assignees: None,
                    body: Some(pr_body),
                    due_date: None,
                    milestone: None,
                    state: None,
                    title: None,
                    unset_due_date: None,
                    allow_maintainer_edit: None,
                    base: None,
                    labels: None,
                },
            )
            .await
            .with_context(|| format!("Couldn't edit PR with ID {pr_number}"))?;
        debug!("Updated pull request {username}/{repo_name}/{pr_number}.");

        for issue_comment in &template.issue_template.comments {
            let body = CreateIssueCommentOption {
                body: issue_comment.body.instantiate(&task_info)?,
                updated_at: None,
            };
            self.forgejo
                .issue_create_comment(username, repo_name, issue_number, body)
                .await?;
        }

        if let Some(pr_template) = &template.pr_template {
            for pr_comment in &pr_template.comments {
                let comment_text = pr_comment.body.instantiate(&task_info)?;
                if let Some(quote) = &pr_comment.quote {
                    // In Forgejo, only one line can have the comment, so only
                    // one of old or new can be set. Since some lines above the
                    // indicated one are displayed and new lines are displayed
                    // after old lines, using the new line number gives the best
                    // result.
                    let (old_line, new_line) = match (&quote.old_line, &quote.new_line) {
                        (None, None) => (None, None),
                        (Some(old), None) => (Some(old.end), None),
                        (_, Some(new)) => (None, Some(new.end)),
                    };
                    debug!("Commenting on {old_line:?}:{new_line:?}.");
                    let body = CreatePullReviewComment {
                        body: Some(comment_text),
                        new_position: new_line,
                        old_position: old_line,
                        path: Some(quote.file.clone()),
                    };
                    let review = CreatePullReviewOptions {
                        body: None,
                        comments: Some(vec![body]),
                        commit_id: hashes.get(&quote.commit.0).cloned(),
                        // This event type is needed to avoid having a review
                        // body and makes it submit the given comment directly.
                        event: Some("COMMENT".to_string()),
                    };
                    self.forgejo
                        .repo_create_pull_review(username, repo_name, pr_number, review)
                        .await?;
                    // Reviews with event type COMMENT are not separately submitted.
                } else {
                    let body = CreateIssueCommentOption {
                        body: comment_text,
                        updated_at: None,
                    };
                    self.forgejo
                        .issue_create_comment(username, repo_name, pr_number, body)
                        .await?;
                }
            }
        }

        Ok(Task {
            issue: Issue {
                number: issue_number,
                url: issue.html_url.with_context(|| {
                    format!("No issue URL for {username}/{repo_name}#{issue_number}")
                })?,
            },
            pr: PullRequest {
                number: pr_number,
                url: pr.html_url.with_context(|| {
                    format!("No issue URL for {username}/{repo_name}#{pr_number}")
                })?,
            },
        })
    }

    async fn hooks_page(&self, page: u32) -> Result<(i64, Vec<Hook>)> {
        let response = self
            .forgejo
            .admin_list_hooks(forgejo_api::structs::AdminListHooksQuery {
                page: Some(page),
                limit: Some(100),
            })
            .await?;
        Ok((response.0.x_total_count.unwrap_or(0), response.1))
    }

    pub async fn register_webhook(&self, url: Url) -> Result<()> {
        let hooks = self.all_pages(|page| self.hooks_page(page)).await?;
        if !hooks
            .iter()
            .any(|hook| hook.url.as_ref().is_some_and(|hook_url| hook_url == &url))
        {
            // if not already registered, then register
            self.forgejo
                .admin_create_hook(forgejo_api::structs::CreateHookOption {
                    active: Some(true),
                    authorization_header: None,
                    branch_filter: Some("*".to_string()),
                    config: CreateHookOptionConfig {
                        content_type: "json".to_string(),
                        url,
                        // not documented, but required to make a system hook instead of a default hook
                        additional: BTreeMap::from([(
                            "is_system_webhook".to_string(),
                            "true".to_string(),
                        )]),
                    },
                    events: Some(vec!["pull_request".to_string()]),
                    r#type: CreateHookOptionType::Forgejo,
                })
                .await?;
        }
        Ok(())
    }
}
