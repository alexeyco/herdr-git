#!/bin/sh
set -eu

script=$(cd "$(dirname "$0")/.." && pwd)/scripts/check-release-version.sh
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT HUP INT TERM

printf 'version = "1.2.3"\n' > "$fixture/Cargo.toml"
printf 'version = "1.2.3"\n' > "$fixture/herdr-plugin.toml"
sh "$script" v1.2.3 "$fixture"

if sh "$script" v1.2.4 "$fixture"; then
    echo "expected a mismatched release tag to fail" >&2
    exit 1
fi

printf 'version = "1.2.4"\n' > "$fixture/herdr-plugin.toml"
if sh "$script" v1.2.3 "$fixture"; then
    echo "expected a mismatched plugin version to fail" >&2
    exit 1
fi

if sh "$script" 1.2.3 "$fixture"; then
    echo "expected a tag without the v prefix to fail" >&2
    exit 1
fi
