#!/usr/bin/env bash
set -euo pipefail

target_triple="${1:?usage: package_macos.sh <target-triple> <arch> <output-dmg>}"
architecture="${2:?usage: package_macos.sh <target-triple> <arch> <output-dmg>}"
output_name="${3:?usage: package_macos.sh <target-triple> <arch> <output-dmg>}"

product_name="Cditor"
bundle_identifier="io.github.jychen8866.cditor"
version="${CDITOR_VERSION:-$(awk -F '"' '/^version = "/ { print $2; exit }' Cargo.toml)}"
binary_path="target/${target_triple}/release/cditor-app"
bundle_root="dist/${architecture}/${product_name}.app"
contents_dir="${bundle_root}/Contents"
macos_dir="${contents_dir}/MacOS"
dmg_root="dist/${architecture}/dmg-root"
output_path="dist/${output_name}"

if [[ ! -x "${binary_path}" ]]; then
  echo "missing release binary: ${binary_path}" >&2
  exit 1
fi

if [[ -e "${bundle_root}" || -e "${dmg_root}" || -e "${output_path}" ]]; then
  echo "packaging output already exists; use a clean dist directory" >&2
  exit 1
fi

install -d "${macos_dir}" "${dmg_root}"
install -m 755 "${binary_path}" "${macos_dir}/${product_name}"

cat > "${contents_dir}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>${product_name}</string>
  <key>CFBundleExecutable</key>
  <string>${product_name}</string>
  <key>CFBundleIdentifier</key>
  <string>${bundle_identifier}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${product_name}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.productivity</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

plutil -lint "${contents_dir}/Info.plist"
codesign --force --deep --sign - "${bundle_root}"
codesign --verify --deep --strict "${bundle_root}"

ditto "${bundle_root}" "${dmg_root}/${product_name}.app"
ln -s /Applications "${dmg_root}/Applications"

hdiutil create \
  -volname "${product_name}" \
  -srcfolder "${dmg_root}" \
  -format UDZO \
  "${output_path}"

checksum="$(shasum -a 256 "${output_path}" | awk '{ print $1 }')"
printf '%s  %s\n' "${checksum}" "${output_name}" > "${output_path}.sha256"

echo "created ${output_path}"
