#!/usr/bin/env bash

set -euo pipefail

IMAGE="5gdescrambler"
REMOTE_IMAGE="friwidev/5gdescrambler:latest"

SCRIPT_DIR="$(dirname $0)"

if ! command -v docker >/dev/null 2>&1; then
    echo "Error: Docker is not installed or not available in PATH."
    exit 1
fi

if ! docker info >/dev/null 2>&1; then
    echo "Error: Cannot access the Docker daemon."
    echo
    echo "Make sure Docker is running and that your user has permission to access the Docker socket."
    exit 1
fi

if [[ ! -d "$SCRIPT_DIR/evaluation" ]]; then
    echo "Error: Evaluation directory not found:"
    echo "  $SCRIPT_DIR/evaluation"
    exit 1
fi

if [[ ! -d "$SCRIPT_DIR/descrambler/samples" ]]; then
    echo "Error: Samples directory not found:"
    echo "  $SCRIPT_DIR/descrambler/samples"
    exit 1
fi

DOCKER_USB_ARGS=()

if [[ -d /dev/bus/usb ]]; then
    DOCKER_USB_ARGS=(
        -v /dev/bus/usb:/dev/bus/usb
    )
else
    echo "Warning: /dev/bus/usb is not available. No USB SDR will be utilizable from within the container."
fi

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "Docker image '$IMAGE' not found locally."

    echo "Attempting to pull $REMOTE_IMAGE..."

    if docker pull "$REMOTE_IMAGE"; then
        echo "Successfully pulled $REMOTE_IMAGE."
        docker tag "$REMOTE_IMAGE" "$IMAGE"
    else
        echo "Failed to pull $REMOTE_IMAGE."
        echo "Building image locally..."

        cd "$SCRIPT_DIR"

        TOTAL_RAM_GB=$(( $(awk '/MemTotal:/ {print $2}' /proc/meminfo) / 1024 / 1024 ))

        if (( TOTAL_RAM_GB < 30 )); then
            echo "Warning: This system has approximately ${TOTAL_RAM_GB} GiB of RAM."
            echo "5GDescrambler is recommended to be compiled with at least 32 GiB of RAM due to rustc codegen consuming large amounts of memory."
            echo

            read -r -p "Do you want to proceed anyway? [y/N] " response

            case "$response" in
                [yY]|[yY][eE][sS])
                    ;;
                *)
                    echo "Aborted."
                    exit 1
                    ;;
            esac
        fi

        if ! docker build --network host -t "$IMAGE" .; then
            echo "Error: Docker build failed."
            exit 1
        fi

        echo "Successfully built '$IMAGE'."
    fi
fi

exec docker run --rm -it \
    -v "$SCRIPT_DIR/evaluation:/app/evaluation" \
    -v "$SCRIPT_DIR/descrambler/samples:/app/descrambler/samples" \
    -v "$SCRIPT_DIR/:/app/src" \
    --network=host \
    "${DOCKER_USB_ARGS[@]}" \
    --privileged \
    "$IMAGE" \
    "$@"
