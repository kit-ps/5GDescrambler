#!/usr/bin/env bash
set -e

FILE="$(dirname $0)/test_dci.rs"

if ! [[ "$1" =~ ^[0-9]+$ ]]; then
    echo "Usage: $0 <RUNS_PER_CONFIG>" >&2
    exit 1
fi

sed -i -E "s/^(const RUNS_PER_CONFIG: usize = )[0-9]+(;)/\1${1}\2/" "$FILE"