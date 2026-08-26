# Grammachy glossary

Terms used in the Grammachy domain. No implementation detail.

- **Check**: one request to find mistakes in one piece of text.
- **Capture**: the act of getting the text the user selected into a Check.
- **Selection**: the text the user highlighted in another application.
- **Issue**: one mistake found by a Check. Has an original span, a proposed fix, and a short reason. The Issues of one Check are ordered by position and never overlap.
- **Fix**: the replacement text for one Issue.
- **Accept**: the user's decision to apply one Fix. **Skip** is the decision to leave the span unchanged.
- **Corrected text**: the Selection with all Accepted Fixes applied.
- **Apply**: delivery of the Corrected text back to the user. Two modes: **Clipboard** (default) and **Auto-replace** (opt-in, overwrites the Selection only).
- **Engine**: the component that performs a Check. Engines are pluggable. Examples: LanguageTool, Harper, a local LLM, the Claude API.
- **Cloud engine**: an Engine that sends the text of a Check to a service outside this machine. Every other Engine keeps the text on the machine. Never the default.
- **Consent**: the user's one-time agreement that a Cloud engine may send text. Given once on a card, kept by the shell, and asked again only if it was never given.
- **Check size limit**: the most text one Check may carry. The limit belongs to the Engine, so each Engine names its own.
- **Native language**: the language the user thinks in. Tunes which mistakes an Engine looks for. A user picks one from a list at a time. **None** is the default and means no tuning.
- **Target English**: the English variant the text is checked against. Default en-US.
- **Depth**: the class of mistake a Check reports. v1 is grammar and spelling only. Style is never reported.
- **Trigger**: the event that starts a Check. v1 triggers are the hotkey and the Compose window.
- **Compose window**: the second v1 surface. A window where the user pastes or types long text and the plugin checks it in Chunks. The Panel hands a Selection over the size limit to it. Shortened to **Compose** on buttons and in the hotkey name.
- **Draft**: the text held in the Compose window. It persists between opens until the user clears it.
- **Chunk**: one slice of a Draft that fits under the Check size limit. A Check of a Draft is one Check per Chunk, and the Issues merge into one list.
- **Panel**: the popup where Issues are shown as marks on the Selection and Accepted or Skipped.
- **Settings**: the user's standing choices that shape every Check: Native language, Target English, Engine, and the Apply mode. Changed inside the Panel, kept by the shell.
- **Edit**: one expected correction in an eval item: a span of the text and the replacement a human annotator gave. An item may carry several Edits or none.
- **Pair**: the match between one Issue and one Edit of the same item. An Issue and an Edit pair when their spans share text and the Issue is not much wider than the Edit. Each Issue and each Edit belongs to at most one Pair.
- **Exact fix**: a Check whose Corrected text, with every Fix Accepted, equals the corrected sentence the eval item expects.
- **Style creep**: an Issue that pairs with no Edit on an item that has Edits. Measures how far an Engine strays past Depth.
- **Valid Check**: a Check that returned a result. A Check that returned an error or timed out is invalid and counts as finding nothing.
- **Eval set**: the sentences with known mistakes that a benchmark ranks Engines against. They are drawn from a licensed learner corpus that is fetched at benchmark time and never committed. They stand beside the **Fixture**, the hand-written sentences committed with the code.
- **Record file**: the readable, per-sentence output of one benchmark run, kept only on the machine that ran it. It is the one place the Eval set text appears beside an Engine's answer.
