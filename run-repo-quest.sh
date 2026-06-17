#!/bin/sh

if ! docker info >/dev/null 2>&1; then
  echo "Error: could not connect to Docker daemon. You might need to start Docker and run this command again."
  exit 1
fi

DOCKER_HOST=$(docker context inspect --format '{{.Endpoints.docker.Host}}')
export RQ_DOCKER_HOST=${DOCKER_HOST#unix://}
docker compose --file oci://ghcr.io/cognitive-engineering-lab/repoquest-compose:latest up --detach --yes
echo "RepoQuest is now up and running here: http://localhost:8085"
echo "To stop the process, run:"
echo "  docker compose --file oci://ghcr.io/cognitive-engineering-lab/repoquest-compose:latest down"
open http://localhost:8085