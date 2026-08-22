# Bundled FFmpeg binaries

Release packaging places one platform-specific `ffmpeg` and `ffprobe` pair in
this directory. The expected names are:

```text
ffmpeg-<target-triple>[.exe]
ffprobe-<target-triple>[.exe]
```

Supported targets currently include `x86_64-apple-darwin`,
`aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
and `x86_64-pc-windows-msvc`. Development builds can override either binary
with `CDITOR_FFMPEG` or `CDITOR_FFPROBE`.
