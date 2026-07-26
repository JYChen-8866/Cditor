#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/../.."

./scripts/dev/check_structure.sh
./scripts/dev/check_dependency_graph.sh
./scripts/dev/check_release_profile.sh
./scripts/dev/test_run_editor_scripts.sh

printf 'Checking formatting...\n'
cargo fmt --all -- --check

printf '\nChecking workspace...\n'
./scripts/dev/check_feature_matrix.sh

printf '\nRunning strict Clippy...\n'
cargo clippy --workspace --all-targets --all-features -- -D warnings

printf '\nRunning workspace tests...\n'
cargo test --workspace
