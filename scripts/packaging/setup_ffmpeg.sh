#!/usr/bin/env bash
set -euo pipefail

target="${1:?usage: setup_ffmpeg.sh <target-triple>}"
output_dir="components/cditor-video/resources/binaries"
work_dir="$(mktemp -d)"
trap 'rm -rf "${work_dir}"' EXIT

case "${target}" in
  aarch64-apple-darwin)
    ffmpeg_url="https://ffmpeg.martin-riedl.de/download/macos/arm64/1783011502_8.1.2/ffmpeg.zip"
    ffmpeg_archive_sha="ef1aa60006c7b77ce170c1608c08d8e4ba1c30c5746f2ac986ded932d0ac2c3c"
    ffmpeg_sha="eaf91238e104dd0e262bc6510e25061855cc99a6955a721b0ac99660d58c473d"
    ffprobe_url="https://ffmpeg.martin-riedl.de/download/macos/arm64/1783011502_8.1.2/ffprobe.zip"
    ffprobe_archive_sha="c39787f4af7a3932502d2d48db6f6feaaa836b48a73ef78c32cc3285df61dfaf"
    ffprobe_sha="ed9dc5871914b466b96b402c9ec0ba68ce4f836e72faa464b1b4e279835bd4a6"
    ;;
  x86_64-apple-darwin)
    ffmpeg_url="https://ffmpeg.martin-riedl.de/download/macos/amd64/1783018342_8.1.2/ffmpeg.zip"
    ffmpeg_archive_sha="a52ef43883f44c219766d4b3bdde4e635b35465d0b704c01c3a0566b59775df9"
    ffmpeg_sha="1ca59dda73668c59898a0b305afd8a88817a989187f222ec62d64e775d614d23"
    ffprobe_url="https://ffmpeg.martin-riedl.de/download/macos/amd64/1783018342_8.1.2/ffprobe.zip"
    ffprobe_archive_sha="5408ca588c8c72b0dde3afe676d0a7acf25ef97e55ae6eba5c7bede1cda42695"
    ffprobe_sha="bdb6aff0f1f414382effd97040f7862dc85e67996ac296cb4288beed0e06498f"
    ;;
  x86_64-unknown-linux-gnu)
    ffmpeg_url="https://ffmpeg.martin-riedl.de/download/linux/amd64/1783011670_8.1.2/ffmpeg.zip"
    ffmpeg_archive_sha="56452c0bfc4ee0325cd615d62f46ba8264f62eed34f727c2224c6c84fa7b8719"
    ffmpeg_sha="bea0dfb96f7223b1be497cf11ccda9ddd9a39103b948b342bb6db1c60a56be12"
    ffprobe_url="https://ffmpeg.martin-riedl.de/download/linux/amd64/1783011670_8.1.2/ffprobe.zip"
    ffprobe_archive_sha="c6f2d36e98f9a4445fad0b0be539f4c4faf13fd502116bf131becd53f56cd390"
    ffprobe_sha="f0a9c3c87d45fe323ae893fe9820150a46f5af9fc5f75066712097f160befac5"
    ;;
  aarch64-unknown-linux-gnu)
    ffmpeg_url="https://ffmpeg.martin-riedl.de/download/linux/arm64/1783010599_8.1.2/ffmpeg.zip"
    ffmpeg_archive_sha="ab9e16864b6bf4ae7e13bbdbdc29621be11a5c547c57af8d4250e9fa2f5e6461"
    ffmpeg_sha="93a3684e7467d33881f8fa39e3b8408248d4f95fb2e9f6b18383edcdbd70f163"
    ffprobe_url="https://ffmpeg.martin-riedl.de/download/linux/arm64/1783010599_8.1.2/ffprobe.zip"
    ffprobe_archive_sha="fb78317b81cdeb614533be59e489019b754afd199670666af28f0e9574be395b"
    ffprobe_sha="7a4103c64cd78c7c634a5610ea3ae5dd3a97b3714cc831407c668decf6a34c6d"
    ;;
  x86_64-pc-windows-msvc)
    shared_url="https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-06-30-13-34/ffmpeg-n8.1.2-21-gce3c09c101-win64-gpl-8.1.zip"
    shared_archive_sha="682361e32c9631caec09e5d9f09077101c9ed90c14e275f62014fefa6d397990"
    ffmpeg_sha="c47e9e15e76897778915ba16e36c8002b0a3f2f9e7c0a71aa1d41459ac1d02d1"
    ffprobe_sha="2864c7a71b820b07d3a9666bb4389c8af4bb9449876b07a75b3b7f15adbdafaa"
    ;;
  *)
    echo "unsupported FFmpeg target: ${target}" >&2
    exit 1
    ;;
esac

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

verify() {
  local path="$1"
  local expected="$2"
  local actual
  actual="$(sha256 "${path}")"
  if [[ "${actual}" != "${expected}" ]]; then
    echo "checksum mismatch for ${path}: expected ${expected}, got ${actual}" >&2
    exit 1
  fi
}

download_one() {
  local tool="$1"
  local url="$2"
  local archive_sha="$3"
  local binary_sha="$4"
  local extension=""
  [[ "${target}" == *windows* ]] && extension=".exe"
  local destination="${output_dir}/${tool}-${target}${extension}"
  if [[ -f "${destination}" ]] && [[ "$(sha256 "${destination}")" == "${binary_sha}" ]]; then
    return
  fi
  local archive="${work_dir}/${tool}.zip"
  curl --fail --location --retry 3 --output "${archive}" "${url}"
  verify "${archive}" "${archive_sha}"
  unzip -o "${archive}" -d "${work_dir}/${tool}" >/dev/null
  local extracted
  extracted="$(find "${work_dir}/${tool}" -type f -name "${tool}${extension}" -print -quit)"
  install -d "${output_dir}"
  install -m 755 "${extracted}" "${destination}"
  verify "${destination}" "${binary_sha}"
}

if [[ -n "${shared_url:-}" ]]; then
  shared_archive="${work_dir}/ffmpeg-shared.zip"
  curl --fail --location --retry 3 --output "${shared_archive}" "${shared_url}"
  verify "${shared_archive}" "${shared_archive_sha}"
  for tool in ffmpeg ffprobe; do
    unzip -j -o "${shared_archive}" "*/bin/${tool}.exe" -d "${work_dir}/${tool}" >/dev/null
    binary="$(find "${work_dir}/${tool}" -type f -name "${tool}.exe" -print -quit)"
    destination="${output_dir}/${tool}-${target}.exe"
    install -d "${output_dir}"
    install -m 755 "${binary}" "${destination}"
    if [[ "${tool}" == "ffmpeg" ]]; then verify "${destination}" "${ffmpeg_sha}"; else verify "${destination}" "${ffprobe_sha}"; fi
  done
else
  download_one ffmpeg "${ffmpeg_url}" "${ffmpeg_archive_sha}" "${ffmpeg_sha}"
  download_one ffprobe "${ffprobe_url}" "${ffprobe_archive_sha}" "${ffprobe_sha}"
fi

echo "FFmpeg runtime ready for ${target}"
