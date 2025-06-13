#!/bin/bash

set -eux

if [ -z "$1" ]; then
    echo "Usage: $0 <version>"
    exit 1
fi

if [[ ! "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Invalid version format (expected semver)"
    exit 1
fi

# docker system prune -f

docker build -t ketikai/github-webhook-rust:$1 .
docker push ketikai/github-webhook-rust:$1

docker tag ketikai/github-webhook-rust:$1 ketikai/github-webhook-rust:latest
docker push ketikai/github-webhook-rust:latest

echo "Successfully released version '$1'"
