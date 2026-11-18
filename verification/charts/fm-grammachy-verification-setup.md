## State chart

This chart shows the quick popup run that this PR puts under verification: the capture, the review, Copy, and Replace.
TLC drew it from a scratch copy of `QuickRun.tla` with one summon, no Clear, and no focus change by you (366 states).
The full check uses three summons and one Clear.

![State chart of the quick popup run: capture, review, and Replace](fm-grammachy-verification-setup.svg)

```mermaid
---
title: "Quick popup: capture, review, and Replace"
---
stateDiagram-v2
  state "Legend" as legend {
    lg0: capture
    lg1: review
    lg2: replace
    lg3: closed
    lg4: Breaks HighlightOutlivesReplace
    lg5: Path to the HighlightOutlivesReplace bug
  }
  class lg0 tan
  class lg1 blue
  class lg2 yellow
  class lg3 teal
  class lg4 breaks
  class lg5 bugPath
  s0: Popup closed
  s1: Hidden, reading the highlight
  s2: Checking
  s3: Hidden, your clipboard is borrowed for Ctrl + C
  s4: Popup closed, highlight released
  s5: Engine error
  s6: Review
  s7: Replace: the fix goes to the clipboard
  s8: Replace: popup closed, the source window takes the keyboard
  s9: Replace: popup closed, the fix goes to the clipboard, highlight released
  s10: Nothing new
  s11: Replace: popup closed, the source window takes the keyboard, highlight released
  s12: Replace: Ctrl + V types the fix
  s13: The source window closed
  s14: Replace: Ctrl + V types the fix, highlight released
  s15: Popup closed, the paste missed the highlight
  [*] --> s0
  s0 --> s1: you press the hotkey
  s1 --> s1: the compositor names the window
  s1 --> s2: the highlight is read
  s1 --> s3: your clipboard is saved
  s2 --> s4: you close it
  s2 --> s5: the engine fails
  s2 --> s6: the engine answers
  s5 --> s4: you close it
  s6 --> s4: you close it
  s6 --> s7: you press Replace
  s7 --> s8: the fix is on the clipboard
  s7 --> s9: you close it
  s3 --> s10: Ctrl + C copies nothing, your clipboard goes back
  s3 --> s2: Ctrl + C copies the selection, your clipboard goes back
  s9 --> s11: the fix is on the clipboard
  s10 --> s0: you close it
  s11 --> s11: the overlay focuses the source window
  s8 --> s12: the source window has the keyboard
  s8 --> s13: the source window is gone
  s11 --> s14: the source window has the keyboard
  s12 --> s4: the paste lands, the highlight is released
  s13 --> s4: you close it
  s11 --> s13: the source window is gone
  s14 --> s15: the paste lands, the highlight is released
  s14 --> s4: the paste lands, the highlight is released
  classDef breaks fill:#ffecee,stroke:#e72e3b,stroke-width:2.5px,color:#cd2834
  class s0 teal
  class s0 bugPath
  class s1 tan
  class s1 bugPath
  class s2 blue
  class s2 bugPath
  class s3 tan
  class s4 teal
  class s5 blue
  class s6 blue
  class s6 bugPath
  class s7 yellow
  class s7 bugPath
  class s8 yellow
  class s9 yellow
  class s9 bugPath
  class s10 blue
  class s11 yellow
  class s11 bugPath
  class s12 yellow
  class s13 blue
  class s14 yellow
  class s14 bugPath
  class s15 teal
  class s15 breaks
  class s15 bugPath
```

What the chart shows:

- "Hidden, reading the highlight" and "Hidden, your clipboard is borrowed for Ctrl + C" are the capture of spec section 3.
  The popup stays hidden until the capture ends, and the borrowed clipboard goes back on each path out.
- "Replace: popup closed, the source window takes the keyboard" is the wait of the Apply table in spec section 6.
  Ctrl + V types only after the compositor names the source window, and the highlight is released after the paste.
- The red state breaks `HighlightOutlivesReplace`, a recorded finding.
  You press Replace, and you close the popup before `wl-copy` ends.
  The close releases the highlight, and the paste lands with no highlight to replace.

The chart does not draw steps that stay in one chart state, such as Ctrl + C, the 150 ms wait, and the clipboard restore.
It does not draw Copy, because Copy stays in "Review".
The two findings about the clipboard and the release need three summons, so this chart does not show them.

Covered digest: `028bf9b3b6302479a4f3458910a69dc299a5f54939148e148b9df94236110a15`
