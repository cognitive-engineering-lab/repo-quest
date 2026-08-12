#!/bin/sh

set -e

if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  ENGINE=docker
elif command -v podman >/dev/null 2>&1 && podman info >/dev/null 2>&1; then
  ENGINE=podman
else
  echo "Error: could not find an active Docker or Podman installation."
  echo "Install Docker (https://docs.docker.com/engine/install/) or Podman (https://podman.io/docs/installation), make sure it's running, and run this command again."
  exit 1
fi

case $ENGINE in
    docker)
	DOCKER_HOST=$(docker context inspect --format '{{.Endpoints.docker.Host}}')
	export RQ_DOCKER_HOST=${DOCKER_HOST#unix://}
	;;
    podman)
	PODMAN_SOCKET=$(podman info --format '{{.Host.RemoteSocket.Path}}' 2>/dev/null)
	export RQ_DOCKER_HOST=${PODMAN_SOCKET#unix://}
	export PODMAN_COMPOSE_WARNING_LOGS=false
	;;
esac

"$ENGINE" compose --file oci://ghcr.io/cognitive-engineering-lab/repoquest-compose:latest up --detach --pull always --yes
echo "RepoQuest is now up and running here: http://localhost:8085"
echo "To stop the process, run:"
echo "  $ENGINE compose --file oci://ghcr.io/cognitive-engineering-lab/repoquest-compose:latest down"

xopen() (
    if [ "$#" -ne 1 ] || [ -z "$1" ]; then
        printf 'usage: xopen <url-or-path>\n' >&2
        exit 2
    fi

    target=$1

    # Local files: canonicalize, so a relative path or a leading '-' can't be
    # misread as a flag by the launcher.
    if [ -e "$target" ]; then
        dir=$(dirname -- "$target") || exit 1
        base=$(basename -- "$target") || exit 1
        cd -- "$dir" || exit 1
        # $PWD is "/" at the root; "//x" is implementation-defined in POSIX.
        case $PWD in
            */) target=$PWD$base ;;
            *) target=$PWD/$base ;;
        esac
    else
        case $target in
            -*)
                printf 'xopen: refusing arg starting with "-": %s\n' "$target" >&2
                exit 2
                ;;
        esac
    fi

    # stdout muted (xdg-open/gio chatter); stderr kept so real failures surface.
    case $(uname -s) in
        Darwin)
            exec open -- "$target" >/dev/null
            ;;
        CYGWIN* | MINGW* | MSYS*)
            # "" is the window-title slot that `start` would otherwise steal.
            exec cmd /c start "" "$target" >/dev/null
            ;;
    esac

    if grep -qi microsoft /proc/sys/kernel/osrelease 2>/dev/null; then
        if command -v wslview >/dev/null 2>&1; then
            exec wslview "$target" >/dev/null
        fi
        exec powershell.exe -NoProfile -Command Start-Process "$target" >/dev/null
    fi

    # No GUI and no $BROWSER means nothing can plausibly handle this.
    if [ -z "$DISPLAY$WAYLAND_DISPLAY$BROWSER" ]; then
        printf 'xopen: no display; not launching a handler for: %s\n' "$target" >&2
        exit 1
    fi

    if command -v xdg-open >/dev/null 2>&1; then
        exec xdg-open "$target" >/dev/null
    elif command -v gio >/dev/null 2>&1; then
        exec gio open "$target" >/dev/null
    fi

    printf 'xopen: no opener found (tried xdg-open, gio)\n' >&2
    exit 127
)

xopen http://localhost:8085
