#!/usr/bin/dumb-init /bin/sh

until curl -s "http://forgejo:3000/healthz"; do
    echo "Waiting for Forgejo..."
    sleep 1;
done

export RQ_PORT=${RQ_PORT:-8085}
exec repo-quest-bot --state-dir /srv/repoquest --public-url "http://localhost:$RQ_PORT/rq"
