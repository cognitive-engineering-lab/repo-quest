# RepoQuest

RepoQuest is an experimental tool for interactive programming tutorials. Each
lesson takes place in a Git repository hosted in a local instance (running in
Docker) of the [Forgejo](https://forgejo.org/) Git forge. RepoQuest uses the
Forgejo interface for issues and pull requests to provide starter code and
explain programming concepts.

## Running RepoQuest

To run RepoQuest using Docker, run

```
./scripts/build-docker.sh
./scripts/run-docker.sh
```

To run RepoQuest using Podman, run

```
./scripts/build-podman.sh
./scripts/run-podman.sh
```

The run scripts will make use of a Docker or Podman volume called
`repo-quest-data` if it exists, and if not, will create it.

Once you have done that, you can access RepoQuest at [https://localhost:3000/].
You will need to register so that RepoQuest knows your email address (for
correctly associating commits). To clone from and push to the RepoQuest
instance, you can either use the username and password you registered with
(e.g., `git clone http://username:password@localhost:3000/username/repo.git`) or
you can register an SSH with RepoQuest key in the user preferences section of
the UI (e.g., `git clone ssh://git@localhost:2222/username/repo.git`).

To start a quest after registering, first [upload a quest
definition bundle](http://localhost:3000) (such as [rqst-async.tgz]()), and then start the
quest.

> [!NOTE]
>
> rqst-async does not currently distribute a quest bundle, but you can
> create one by following [the bundling steps below](#bundling-a-quest).

## Quest development

RepoQuest quest definitions are GitHub repositories that follow certain
conventions. The [rqst-async quest
definition](https://github.com/cognitive-engineering-lab/rqst-async/) is a good
example of a quest definition.

The quest structure is defined by a file called `meta.toml` on the `meta` branch
of the repository. For example,

```toml
title = "Async Rust"
author = "cognitive-engineering-lab"
repo = "rqst-async"
rq-version = "0.3.0"

[[chapters]]
label = "00-chat-route"
name = "Warmup"
no-starter = true

[[chapters]]
label = "01-async-await"
name = "Async and Await"
```

The labels correspond to branch names, such as `01-async-await-a`, which
contains the scaffolding for the chapter, and `01-async-await-b`, which contains
the reference solution for the chapter.

The `no-starter` flag indicates that there is no additional scaffolding code to
add from the previous quest's solution. In that case, there is no scaffolding
branch. E.g., there is no `00-chat-route-a` branch, only a `00-chat-route-b`
branch.

Each chapter must have exactly one corresponding GitHub issue that is labeled
with the chapter label. For example, see the issue corresponding to
[00-chat-route](https://github.com/cognitive-engineering-lab/rqst-async/issues/16).

The title, body, and comments on each issue are recreated in the learner's quest
repository, authored by the "repoquest" user.

Each chapter must have zero or one corresponding GitHub pull requests
corresponding to a chapter. Whether a pull request corresponds will be
determined by whether there is a pull request with the scaffolding branch of a
chapter as the source branch. The target of the pull request must be the
reference solution branch of the previous chapter, or `main` for the first
chapter. See, for example, the pull request for chapter
[01-async-await](https://github.com/cognitive-engineering-lab/rqst-async/pull/17).

The title, body, and comments on each issue are recreated in the learner's quest
repository, authored by the "repoquest" user. Review comments that quote code
are also recreated, but due to limitations in Forgejo, only the end line for
each quote is preserved.

Labels on pull requests are not used by RepoQuest, despite appearing in the
`rqst-async` quest example.

Pull requests from reference solution branches into scaffolding branches are not
currently used by RepoQuest, but may be in the future.

The `main` branch is used as the initial contents of the repository. The history
of the `main` branch is not included in the repository of the learner's quest.
Instead, the history is squashed and given the message "Initial commit".

Unlike the history of `main`, the history between reference solution branches
and the following scaffolding branch will be preserved in a future version of
RepoQuest.

### Bundling a quest

In order to use a quest definition with RepoQuest it has to be bundled. This can
be done using the `repo-quest-bundle` binary. For example,

```sh
cargo run --bin repo-quest-bundle -- \
    --owner cognitive-engineering-lab \
    --repo rqst-async \
    --output rqst-async.tgz \
    --token "$GH_TOKEN"
```

For small quests in public repositories, providing a GitHub authentication token
might not be necessary. However, the rate limits on unauthenticated requests to
the GitHub API are very low, so we recommend using a token. For a public
repository, a fine-grained access token with no permissions can be used.

The resulting bundle is independent of GitHub and can be installed as a quest
definition in RepoQuest.

## RepoQuest development

In addition to ports 3000 and 2222 for HTTP and SSH, port 8000 (on which runs
the RepoQuest bot) is also exposed from the Docker container.
