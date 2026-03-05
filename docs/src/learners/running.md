# Running RepoQuest

To run RepoQuest using Docker, in the root of this repository run

```sh
docker compose up --build --detach
```

To run RepoQuest using Podman, in the root of this repository run

```sh
podman compose up --build --detach
```

You can control the port that RepoQuest uses for its HTTP server with the
`RQ_PORT` environment variable and the port used for its SSH server with the
`RQ_SSH_PORT` environment variable. For example,

```sh
RQ_PORT=8000 RQ_SSH_PORT=2022 docker compose up --build --detach
```

Once that command returns successfully, you can access RepoQuest at
[http://localhost:8085/](http://localhost:8085/). You will need to register so
that RepoQuest knows your email address (for correctly associating commits). To
clone from and push to the RepoQuest instance, you can either use the username
and password you registered with (e.g., `git clone
http://username:password@localhost:3000/username/repo.git`) or you can register
an SSH with RepoQuest key in the user preferences section of the UI (e.g., `git
clone ssh://git@localhost:2222/username/repo.git`).

To shut down RepoQuest, run `podman compose down` in the same directory. Docker
or Podman volumes associated with the compose service will persist your
configuration and the quest data.

To start a quest after registering, first [upload a quest definition
bundle](http://localhost:8085) (such as [rqst-async.tgz](#TODO)), and then start
the quest.

> [!NOTE]
>
> rqst-async does not currently distribute a quest bundle, but you can
> create one by following [the bundling steps below](#bundling-a-quest).

### Forgejo Actions CI support

In order to support Forgejo Actions, the socket for communicating with Docker or
Podman must be made available to the Forgejo Runner container. For Docker the
default configuration should work with no changes. For rootless Podman, you will
have to start the service that provides the socket and then specify the path to
the socket via the environment variable `RQ_DOCKER_HOST`.

For example,

```sh
systemctl --user start podman.socket
RQ_DOCKER_HOST="$XDG_RUNTIME_DIR/podman/podman.sock" \
    RQ_PORT=3000 \
    podman compose up --build --detach
```
