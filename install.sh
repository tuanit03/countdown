#!/bin/sh
# Install from this checkout: build the image locally, then run the same installer the
# published image ships (`docker run --rm pythonking11/countdown install | sh`).
# Usage: ./install.sh | ./install.sh --uninstall
set -eu
dir=$(cd "$(dirname "$0")" && pwd)
image=countdown:latest
if [ "${1:-}" = "--uninstall" ]; then
	docker run --rm "$image" uninstall | COUNTDOWN_IMAGE="$image" sh
	exit 0
fi
docker build -t "$image" --build-arg IMAGE_REF="$image" "$dir"
docker run --rm "$image" install | COUNTDOWN_IMAGE="$image" sh
