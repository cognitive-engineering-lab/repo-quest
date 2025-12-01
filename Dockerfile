FROM rust:alpine as rq-bot-build

RUN apk add --no-cache \
    openssl-dev \
    openssl-libs-static \
    musl-dev
ADD ./ /root/app
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/root/app/target/ \
    cd /root/app && \
    cargo build --release && \
    cp target/release/repo-quest-bot /root/app/repo-quest-bot

FROM codeberg.org/forgejo/forgejo:13-rootless

# Set up the CI runner
USER root
RUN apk add \
    podman \
    podman-docker \
    nodejs \
    npm \
    openssl \
    forgejo-runner
RUN mkdir -p /data && \
    chown -R 1000:1000 /data

# Switch back to git user
USER 1000:1000

COPY --from=rq-bot-build /root/app/repo-quest-bot /usr/local/bin/repo-quest-bot
ADD --chmod=644 app.ini /etc/templates/app.ini
ADD --chmod=755 run-repoquest.sh /usr/local/bin/run-repoquest.sh
ADD --chown=1000:1000 custom/templates "$GITEA_CUSTOM/templates"

ENTRYPOINT ["run-repoquest.sh"]
