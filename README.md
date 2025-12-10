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

## RepoQuest development

In addition to ports 3000 and 2222 for HTTP and SSH, port 8000 (on which runs
the RepoQuest bot) is also exposed from the Docker container.
