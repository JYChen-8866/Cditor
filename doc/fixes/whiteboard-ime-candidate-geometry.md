# Whiteboard IME Candidate Geometry

## Symptom

On macOS, composing CJK text in a fullscreen whiteboard placed the candidate
window at the lower-left edge of the screen. The document editor did not have
the same failure after its composition geometry path was fixed.

## Root Cause

The fullscreen whiteboard had multiple platform-input ownership paths:

1. `DrafftBoardView::render` already rendered a full-surface input element.
2. The host overlay rendered another full-size input element after the board.
3. `CditorV2View` also contained forwarding methods for whiteboard IME calls.

GPUI stores every input handler registered during paint and installs the last
registered handler for the focused path. The overlay input element therefore
won by paint order. It was a normal-flow `100% x 100%` sibling of the board,
so it also changed layout and received an origin that was not the canvas
origin. Whiteboard candidate geometry then added that origin to camera-local
caret geometry. macOS received an off-screen rectangle and clamped the
candidate window to a screen edge.

This was an ownership and coordinate-space bug. Returning a fallback rectangle
at the host boundary would only hide it and would retain nondeterministic input
ownership.

## Ownership Contract

Each editable text surface must have exactly one platform input owner per
frame.

For a whiteboard:

```text
DrafftBoardView or legacy WhiteboardView
  -> internal absolute full-surface input element
  -> ElementInputHandler<that same board entity>
  -> current composition text
  -> current text layout
  -> current camera transform
  -> the same surface bounds used by canvas paint
  -> window-coordinate candidate rectangle
```

The editor host owns lifecycle and persistence only. It must not proxy the
whiteboard's text-input protocol or append another hidden input element.

## Geometry Contract

Whiteboard text geometry is computed synchronously from the current shape. The
candidate rectangle uses the same transform as paint:

```text
local caret rect
  -> text origin offset
  -> text position
  -> shape rotation
  -> camera pan/zoom
  -> whiteboard surface origin
  -> window-coordinate rect
```

The surface origin must come from the input element colocated with the canvas.
Document coordinates, screen coordinates, and a host sibling's layout bounds
must never be mixed.

## Diagnostics

Run with `CDITOR_TRACE_INPUT=1`. A healthy composition emits one owner family
and a non-empty rectangle inside its surface:

```text
[cditor][input][whiteboard][owner.registered] surface=...
[cditor][input][whiteboard][bounds_for_range] ... result=Some(...)
```

Repeated `owner.registered` lines across frames are normal. More than one
whiteboard owner registration in one frame is not.

## Regression Coverage

- UTF-16 conversion covers CJK and surrogate pairs.
- Platform text input updates the authoritative Drafft shape.
- A marked composition preview returns non-zero window bounds.
- Advancing the composition caret advances candidate x.
- Candidate origin remains inside the owned whiteboard surface.
- The editor test suite verifies that document IME behavior remains unchanged.
