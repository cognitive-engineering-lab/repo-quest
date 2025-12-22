#!/usr/bin/dumb-init /bin/sh

until curl -s "http://forgejo:3000/healthz"; do
    echo "Waiting for Forgejo..."
    sleep 1;
done

forgejo-runner create-runner-file --instance "http://forgejo:3000" --secret "0123456789012345678901234567890123456789"

echo "Starting..."
exec forgejo-runner daemon
