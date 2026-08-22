# Frame Attribution

The architecture of this component is derived from the Frame preview engine:

- Project: https://github.com/66HEX/frame
- Source revision: `2ccdb5d4f4ec29f54d2b710d22e7e4934451680a`
- License: GPL-3.0-or-later

The extracted design covers FFmpeg raw BGRA frame streaming and PCM audio,
CPAL output, latest-frame publication, GPUI `RenderImage` conversion, playback
commands, bounded diagnostics, and process lifecycle management. It was adapted
to Cditor's pinned Zed GPUI dependency and separated from Frame's conversion
application state.
