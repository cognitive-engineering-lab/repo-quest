#!/usr/bin/dumb-init /bin/sh
register() {
    until curl -s "http://localhost:3000/healthz"; do
        sleep 1;
    done

    forgejo admin user create \
            --admin \
            --username repoquest \
            --password repoquest \
            --email repoquest@example.com
    echo "Registered admin user 'repoquest' with Forgejo."

    forgejo admin user create \
            --username hero \
            --password repoquest \
            --email hero@example.com \
    # NOTE(2026-06-11): --must-change-password false does not seem to be
    # working, necessitating the command below.
    forgejo admin user must-change-password --unset hero
    echo "Registered user 'hero' with Forgejo."

    # The runner labels will be set by the runner when it connects.
    forgejo forgejo-cli actions register \
            --secret "0123456789012345678901234567890123456789"
    echo "Pre-registered runner with Forgejo."

    touch "$(dirname ${GITEA_APP_INI})/configured"
}

if [ ! -f "$(dirname ${GITEA_APP_INI})/configured" ]; then
    export SECRET_KEY=$(forgejo generate secret SECRET_KEY)
    export FORGEJO__SERVER__LFS_JWT_SECRET=$(forgejo generate secret JWT_SECRET)
    export FORGEJO__SECURITY__INTERNAL_TOKEN=$(forgejo generate secret INTERNAL_TOKEN)
    export FORGEJO__OAUTH2__JWT_SECRET=$(forgejo generate secret JWT_SECRET)

    register &
fi

# The templates live on the volume, so they need to be overidden on start-up,
# otherwise they don't get updated when a new image is built.
mkdir -p "$GITEA_CUSTOM/templates"
cp -R /etc/templates/templates "$GITEA_CUSTOM/"

# TODO: app.ini has the same problem, but it created only on the volume during
# first run of docker-entrypoint.sh, so recreating it isn't as straightforward.

RQ_PORT="${RQ_PORT:-8085}"
export GITEA__SERVER__ROOT_URL="http://localhost:$RQ_PORT"
RQ_SSH_PORT="${RQ_SSH_PORT:-2222}"
export GITEA__SERVER__SSH_PORT="$RQ_SSH_PORT"
exec /usr/local/bin/docker-entrypoint.sh
