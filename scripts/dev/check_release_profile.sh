#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/../.."

release_profile=$(
  awk '
    /^\[profile\.release\]$/ { in_release = 1; next }
    /^\[/ { in_release = 0 }
    in_release {
      gsub(/[[:space:]]/, "")
      if ($0 != "" && substr($0, 1, 1) != "#") print
    }
  ' Cargo.toml
)

require_setting() {
  setting="$1"
  if ! printf '%s\n' "$release_profile" | grep -F -x "$setting" >/dev/null; then
    echo "error: release profile must contain ${setting}" >&2
    exit 1
  fi
}

require_setting 'opt-level=3'
require_setting 'debug=false'
require_setting 'strip="symbols"'
require_setting 'debug-assertions=false'
require_setting 'overflow-checks=false'
require_setting 'lto="fat"'
require_setting 'panic="abort"'
require_setting 'incremental=false'
require_setting 'codegen-units=1'

if grep -F 'target-cpu=native' .github/workflows/desktop-builds.yml >/dev/null; then
  echo 'error: distributed desktop artifacts must not target the CI host CPU' >&2
  exit 1
fi

if ! grep -F 'target-feature=+crt-static' .github/workflows/desktop-builds.yml >/dev/null; then
  echo 'error: the Windows release must keep the static CRT target feature' >&2
  exit 1
fi

echo 'Release performance profile checks passed.'
