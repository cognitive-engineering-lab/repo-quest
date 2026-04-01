# RepoQuest

RepoQuest is an experimental tool for interactive programming tutorials. Each
lesson takes place in a Git repository hosted in a local instance (running in
Docker) of the [Forgejo](https://forgejo.org/) Git forge. RepoQuest uses the
Forgejo interface for issues and pull requests to provide starter code and
explain programming concepts.

For instructions on running a quest using RepoQuest, see [the documentation for
learners](https://cel.cs.brown.edu/repo-quest/learners.html).

For instructions on authoring a quest, see [the documentation for
authors](https://cel.cs.brown.edu/repo-quest/authors.html).

The remainder of this README is intended for RepoQuest developers.

## RepoQuest development

RepoQuest consists of two binaries, some shared library code, and a Docker
compose configuration.

### Source code organization

One binary (`repo-quest`) is the tool for authoring quests. The other
(`repo-quest-bot`) is the service that runs to support the customized version of
Forgejo. For historical reasons, the Rust code is organized so that the
code specific to the authoring tool is contained in the directory for its binary
while the library contains both the share code and the bot-specific code.

There are two things called "quest definitions" in the source. One corresponds
to the bundled quest. The other corresponds to the authoring tool. The two
variants exist for two reasons:

1. The version used by the bundled quest prefers configuration over convention,
   while the version for the authoring tool prefers convention over
   configuration. This difference reflects the different users of the formats
   (human vs program).
2. We anticipate the representation that is used for the authoring tool needing
   to change more frequently than the representation used in the bundle. By
   having separate representations, old quest bundles are more likely to
   continue to work with newer versions of RepoQuest, even if the quest cannot
   be bundled again without changing the definition to match changes in
   RepoQuest.

### Use of Git and rsync

Because the git porcelain command are more convenient for the operations being
performed and because rsync has the desired behaviors already, RepoQuest invokes
Git and rsync as binaries instead of using the corresponding libraries. Because
the invoked commands are logged on error, this also makes it easier to debug the
implementation if there are problems.

### Single-user

The `repo-quest-bot` service does not perform do any authentication or
authorization checks, or even distinguish between Forgejo users. RepoQuest is
intended for local, single-user use on a trusted machine and so we did not
address any of the security concerns that would be involved in networked or
multi-user use.

Also because of the single-user design, the bot service locks the entire state
of the service on each request. With a single user and no requests to services
other than the local Forgejo instance, there should be minimal contention for
the lock. This does mean that long-running tasks (such as ones supporting
WebSocket connections to the client) need to take care not to hold the lock in
the same way that synchronous HTTP requests do, since that would cause HTTP
requests to block on the long-running tasks.

### The Docker compose configuration

The Docker compose configuration relies on custom images for Forgejo, Forgejo
runner, and RepoQuest and uses an off-the-shelf image for nginx (as a reverse
proxy that dispatches to the Forgejo and RepoQuest images). The Dockerfiles are
given in the `docker` directory and the compose file is in the root of this
repository.

The Dockerfile and the compose file use the `latest` tag for the Rust and nginx
images, and so are somewhat fragile. The following tags are known to work:

- `docker.io/nginxinc/nginx-unprivileged:1.29.5-trixie`
- `docker.io/library/rust:1.94.1-alpine3.23`

Unless overridden by command line options, ports 8085 and 2222 are published
from the container. 8085 is managed by an nginx reverse proxy that redirects
requests to paths beginning with /repoquest/ to port 8000 internally, which the
RepoQuest bot listens on. The remaining requests are redirected to port 3000
internally, which Forgejo listens on. Port 2222 is used for SSH.

### Forgejo UI customization

Forgejo's UI is modified by adding the files under `docker/custom/templates/`.
The customization makes use things that are not part of Forgejo's external
documentation, and so may break with upgrades to the Forgejo image. For example,

- CSS variables and classes defined by Forgjeo are used in custom UI components,
- the content of the user dashboard is overridden, and
- an absolute-positioned sidebar is added as part of `extra_tabs.tmpl`.

When developing the UI without requiring changes to the backend it can be
helpful to just copy in the UI files and then force Forgejo to reload them.
However, those files will be stored in the volume and so the volume will have to
be recreated when actually updating the Forgejo image.

```shell
podman cp docker/custom/templates/ repo-quest_forgejo_1:/var/lib/gitea/custom/ \
    && podman exec repo-quest_forgejo_1 forgejo manager reload-templates
```

### Testing

There is very little in the way of automated tests. The skeleton directory used
for the `init` command serves as a useful minimal case for manual tests.
