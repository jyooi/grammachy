# Grammachy glossary

Terms used in the Grammachy domain. No implementation detail.

- **Check**: one request to find mistakes in one piece of text.
- **Capture**: the act of getting the text the user selected into a Check.
- **Selection**: the text the user highlighted in another application.
- **Issue**: one mistake found by a Check. Has an original span, a proposed fix, and a short reason.
- **Fix**: the replacement text for one Issue.
- **Accept**: the user's decision to apply one Fix. **Skip** is the decision to leave the span unchanged.
- **Corrected text**: the Selection with all Accepted Fixes applied.
- **Apply**: delivery of the Corrected text back to the user. Two modes: **Clipboard** (default) and **Auto-replace** (opt-in, overwrites the Selection only).
- **Engine**: the component that performs a Check. Engines are pluggable. Examples: LanguageTool, Harper, a local LLM, the Claude API.
- **Native language**: the language the user thinks in. Tunes which mistakes an Engine looks for. A user picks one from a list at a time.
- **Target English**: the English variant the text is checked against. Default en-US.
- **Depth**: the class of mistake a Check reports. v1 is grammar and spelling only. Style is never reported.
- **Trigger**: the event that starts a Check. v1 trigger is the hotkey. A later trigger is the Compose window.
- **Compose window**: a later feature where the user types inside the plugin and the Check runs after a pause.
- **Panel**: the surface where Issues are shown and Accepted or Skipped.
