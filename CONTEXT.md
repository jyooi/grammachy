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
- **Engine**: the component that performs a Check. Engines are pluggable. Examples: LanguageTool, Harper.
- **Component**: a piece of an Engine that is fetched and unpacked onto this machine rather than shipped in the binary. The user adds one from Settings and takes it away again, without a password. LanguageTool is the one Component today; Harper needs none, which is why Harper is the default Engine.
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
