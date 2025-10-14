# RepoQuest

## Rootless Podman

Need to make the socket available at /var/run/docker.sock so that
`forgejo-runner` can use it. Also need to make sure it is accessible to
`forgejo-runner`. This is done by mapping it to the `forgejo` account (UID 1000,
GID 1000).

The GUI is hosted on port 3000, ssh (required for git access) on 2222, and the
repo-quest service on 8000 so those ports are exposed on the loopback address.

```console
$ ./scripts/run.sh
```

Then go to http://localhost:3000/

> [!CAUTION]
> Removing the container will delete the state of the system, losing your code
> and your place in the tutorial. To preserve the system state, mount a volume
> to /var/lib/gitea.
