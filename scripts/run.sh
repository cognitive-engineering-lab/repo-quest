#!/bin/sh

# Create podman socket.
socket=$(mktemp --tmpdi="$XDG_RUNTIME_DIR" -u podman-XXXXXX.sock)
podman system service -t 0 "unix:///$socket" &
pid=$!

# Kill socket process on exit
trap "kill \"$!\"" 0

podman run \
    --rm \
    -it \
    --publish 127.0.0.1:3000:3000/tcp \
    --publish 127.0.0.1:8000:8000/tcp \
    --publish 127.0.0.1:2222:2222/tcp \
    --mount=type=bind,source="$socket",destination=/var/run/docker.sock \
    --userns keep-id:uid=1000,gid=1000 \
    --name repoquest \
    repoquest:latest
