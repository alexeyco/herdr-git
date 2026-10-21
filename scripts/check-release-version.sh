#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: check-release-version.sh v<version>" >&2
    exit 2
fi

tag=$1
case "$tag" in
    v?*) version=${tag#v} ;;
    *) echo "release tag must start with v: $tag" >&2; exit 2 ;;
esac

root=${2:-.}
cargo_version=$(grep -m1 '^version = "' "$root/Cargo.toml" | sed -E 's/^version = "(.*)"$/\1/')
plugin_version=$(grep -m1 '^version = "' "$root/herdr-plugin.toml" | sed -E 's/^version = "(.*)"$/\1/')

if [ "$version" != "$cargo_version" ]; then
    echo "release tag $tag does not match Cargo.toml version $cargo_version" >&2
    exit 1
fi
if [ "$version" != "$plugin_version" ]; then
    echo "release tag $tag does not match herdr-plugin.toml version $plugin_version" >&2
    exit 1
fi

echo "release tag $tag matches Cargo.toml and herdr-plugin.toml"
