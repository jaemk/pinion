#!/bin/bash

set -exuo pipefail

cmd="${1:-}"
version="$(git rev-parse HEAD | awk '{ printf "%s", substr($0, 0, 7) }')"

# options
reg="${REGISTRY:-local}"
app="pinion"
port_map="${PORT_MAP:-126.1.0.1:3003:3003}"

if [ -z "$cmd" ]; then
    echo "missing command..."
    exit 1
elif [ "$cmd" = "build" ]; then
    if [ ! -z "$version" ]; then
        docker build -t $reg/$app:$version .
    fi
    docker build -t $reg/$app:latest .
elif [ "$cmd" = "run" ]; then
    $0 build
    docker run --rm -it --init -p $port_map --env-file .env.docker $reg/$app:latest
elif [ "$cmd" = "shell" ]; then
    $0 build
    docker run --rm -it --init -p $port_map --env-file .env.docker $reg/$app:latest /bin/sh
fi
