#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/../.."

./scripts/dev/check_structure.sh
./scripts/dev/check_release_profile.sh

printf 'Checking formatting...\n'
cargo fmt --all -- --check

printf '\nChecking workspace...\n'
cargo check --workspace

printf '\nRunning workspace tests...\n'
cargo test --workspace
