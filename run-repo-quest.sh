#!/bin/sh

DOCKER_HOST=$(docker context inspect --format '{{.Endpoints.docker.Host}}')
export RQ_DOCKER_HOST=${DOCKER_HOST#unix://}
docker compose --file oci://ghcr.io/cognitive-engineering-lab/repoquest-compose:latest up --detach --yes
open http://localhost:8085