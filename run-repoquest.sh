#!/usr/bin/dumb-init /bin/sh

until curl -s "http://forgejo:3000/healthz"; do
    echo "Waiting for Forgejo..."
    sleep 1;
done

exec repo-quest-bot --state-dir /srv/repoquest
