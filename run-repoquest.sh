#!/usr/bin/dumb-init /bin/sh
if [ ! -f ${GITEA_APP_INI} ]; then
    FIRSTRUN=true
    export SECRET_KEY=$(forgejo generate secret SECRET_KEY)
    export FORGEJO__SERVER__LFS_JWT_SECRET=$(forgejo generate secret JWT_SECRET)
    export FORGEJO__SECURITY__INTERNAL_TOKEN=$(forgejo generate secret INTERNAL_TOKEN)
    export FORGEJO__OAUTH2__JWT_SECRET=$(forgejo generate secret JWT_SECRET)
    mkdir -p "$HOME"
    git config --global user.name "RepoQuest"
    git config --global user.email "repoquest@example.com"
fi

/usr/local/bin/docker-entrypoint.sh &

until curl -s http://localhost:3000/healthz; do
    echo "Forgejo is unavailable - sleeping";
    sleep 1;
done

if [ -n "$FIRSTRUN" ]; then
    forgejo admin user create --admin --username repoquest --password repoquest --email repoquest@example.com
    token=$(forgejo forgejo-cli actions generate-runner-token)
    forgejo-runner register --no-interactive --instance http://localhost:3000 --token "$token" --labels=docker
fi

repo-quest-bot repoquest &

exec forgejo-runner daemon
