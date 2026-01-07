#!/usr/bin/dumb-init /bin/sh

# This isn't required when running via compose, since the health checks should
# take care of it. It is useful when running forgejo-runner containers manually
# for debugging, though.
until curl -s "http://forgejo:3000/healthz"; do
    echo "Waiting for Forgejo..."
    sleep 1;
done

# Need to specify the config to set the runner labels are used. Setting them in
# the .runner file didn't work: they get overridden by the default value of []
# in the default configuration.
#
# Also needed to specify the compose network that started containers should be
# part of so that they can reach the other services (e.g., Forgejo).
cp /etc/forgejo-runner.yml ./config.yml

# The "secret" defined here is the same as the one used to create the
# registration in the Forgejo container.
forgejo-runner create-runner-file \
               --instance "http://forgejo:3000" \
               --secret "0123456789012345678901234567890123456789" \
               --config ./config.yml

echo "Starting..."
export DOCKER_HOST=unix:///var/run/docker.sock
exec forgejo-runner daemon --config ./config.yml
