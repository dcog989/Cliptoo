#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: sync_version.sh <version>}"

# Workspace version lives in the root Cargo.toml; crate manifests use
# `version.workspace = true` so they need no edits.
sed -i "s/^version = \".*\"/version = \"$version\"/" Cargo.toml

# Update both local packages in Cargo.lock (cliptoo, cliptoo-core).
for pkg in cliptoo cliptoo-core; do
    sed -i "/^name = \"$pkg\"/{n;s/^version = \".*/version = \"$version\"/}" Cargo.lock
done

sed -i "s/^pkgver=.*/pkgver=$version/" packaging/PKGBUILD
