# Starting the RepoQuest environment

You have a few options for starting RepoQuest, ordered by complexity.

## 1. `curl | sh`

The quickest way to get started is to run our provided shell script:

```sh
curl -sSl https://raw.githubusercontent.com/cognitive-engineering-lab/repo-quest/refs/heads/main/run-repo-quest.sh | sh
```

## 2. Compose with remote script

RepoQuest is run through a Compose script, which can be managed by either Docker or Podman. We will use `docker` in this book, but the commands should be interchangeable for `podman`.

First, you will need to set an environment variable telling RepoQuest where your Docker socket lives (for use in running sub-containers in Forgejo's CI):

```sh
export DOCKER_HOST=$(docker context inspect --format '{{.Endpoints.docker.Host}}')
export RQ_DOCKER_HOST=${DOCKER_HOST#unix://}
```

Second, you will need to run `compose up` using the Compose script uploaded to the container registry:

```sh
docker compose up --file oci://ghcr.io/cognitive-engineering-lab/repoquest-compose:latest
```

Note that you can replace `latest` with a particular version of RepoQuest such as `v0.1.0`.

## 3. Compose from source

Alternatively, if you have cloned the RepoQuest source, you can build and the images locally. In the root of the repository, run:

```sh
export DOCKER_HOST=$(docker context inspect --format '{{.Endpoints.docker.Host}}')
export RQ_DOCKER_HOST=${DOCKER_HOST#unix://}
docker compose up --build
```

## RepoQuest options

You can control the port that RepoQuest uses for its HTTP server with the
`RQ_PORT` environment variable and the port used for its SSH server with the
`RQ_SSH_PORT` environment variable. For example,

```sh
RQ_PORT=8000 RQ_SSH_PORT=2022 docker compose up --build --detach
```

Once that command returns successfully, you can access RepoQuest at
[http://localhost:8085/](http://localhost:8085/) (adjusting the port number
according to how you configured it).

You will need to register so that RepoQuest knows your email address (for
correctly associating commits). To clone from and push to the RepoQuest
instance, you can either use the username and password you registered with
(e.g., `git clone http://username:password@localhost:3000/username/repo.git`) or
you can register an SSH with RepoQuest key in the user preferences section of
the UI (e.g., `git clone ssh://git@localhost:2222/username/repo.git`).

To shut down RepoQuest, run `podman compose down` in the same directory. Docker
or Podman volumes associated with the compose service will persist your
configuration and the quest data.

To start a quest after registering, first [upload a quest definition
bundle](http://localhost:8085) (such as [rqst-async.tgz](#TODO)), and then start
the quest.
