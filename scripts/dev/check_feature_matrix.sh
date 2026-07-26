#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/../.."

echo 'Checking workspace default features...'
cargo check --workspace --all-targets

echo 'Checking workspace without default features...'
cargo check --workspace --all-targets --no-default-features

echo 'Checking all supported workspace features...'
cargo check --workspace --all-targets --all-features

echo 'Feature matrix checks passed.'
