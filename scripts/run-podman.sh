#!/bin/sh

usage() {
    echo "Run the RepoQuest environment using Podman."
    echo "  -h display this help message"
    echo "  -v provide the name of the Podman volume to use (default repo-quest-data)"
    echo "  -r remove and re-create the RepoQuest Podman volume"
    exit
}

if ! command -v podman; then
    echo "podman does not appear to be installed."
    exit 1
fi

volume="repo-quest-data"

while getopts ":hrv:" o; do
    case "${o}" in
        r)
            recreate_volume=1
            ;;
        v)
            volume=${OPTARG}
            ;;
        *)
            usage
            ;;
    esac
done

# Create podman socket.
socket=$(mktemp --tmpdi="$XDG_RUNTIME_DIR" -u podman-XXXXXX.sock)
podman system service -t 0 "unix:///$socket" &
pid=$!

# Kill socket process on exit
trap "kill \"$pid\"" 0

if [ -n "$recreate_volume" ] && podman volume exists "$volume"; then
    echo "Removing volume $volume"
    podman volume rm "$volume"
fi

if ! podman volume exists "$volume"; then
    echo "Creating new podman volume $volume"
    podman volume create "$volume"
else
    echo "Using existing podman volume $volume"
fi

podman run \
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
