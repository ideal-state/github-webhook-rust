#!/bin/bash

set -eux

args=("github-webhook-rust")

if [ -n "$GWR_HOSTNAME" ]; then
    args+=("--hostname" "$GWR_HOSTNAME")
fi

if [ -n "$GWR_PORT" ]; then
    args+=("--port" "$GWR_PORT")
fi

if [ "$GWR_TLS" = "true" ]; then
    args+=("--tls")
fi

if [ -n "$GWR_WORKERS" ]; then
    args+=("--workers" "$GWR_WORKERS")
fi

exec "${args[@]}"
