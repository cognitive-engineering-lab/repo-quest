#!/bin/sh

usage() {
    echo "Run the RepoQuest environment using Docker."
    echo "  -h display this help message"
    echo "  -s provide the path to the Docker socket (default /var/run/docker.sock)"
    echo "  -v provide the name of the Docker volume to use (default repo-quest-data)"
    echo "  -r remove and re-create the RepoQuest Docker volume"
    exit
}

if ! command -v docker; then
    echo "docker does not appear to be installed."
    exit 1
fi

volume="repo-quest-data"
socket="/var/run/docker.sock"

while getopts ":hrvs:" o; do
    case "${o}" in
        r)
            recreate_volume=1
            ;;
        v)
            volume=${OPTARG}
            ;;
        s)
            socket=${OPTARG}
            ;;
        *)
            usage
            ;;
    esac
done

if [ -n "$recreate_volume" ] && docker volume exists "$volume"; then
    echo "Removing volume $volume"
    docker volume rm "$volume"
fi

if ! docker volume exists "$volume"; then
    echo "Creating new podman volume $volume"
    docker volume create "$volume"
else
    echo "Using existing podman volume $volume"
fi

docker run \
    --rm \
    -it \
    --publish 127.0.0.1:3000:3000/tcp \
    --publish 127.0.0.1:8000:8000/tcp \
    --publish 127.0.0.1:2222:2222/tcp \
    --mount=type=volume,source="$volume",destination=/var/lib/gitea \
    --mount=type=bind,source="$socket",destination=/var/run/docker.sock \
    --userns keep-id:uid=1000,gid=1000 \
    --name repoquest \
    --env RUST_LOG \
    repoquest:latest
