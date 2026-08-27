//! One module per engine adapter, plugged into the seam in [`crate::engine`].
//!
//! [`install`] is the odd one out: it is not an adapter but the catalogue of
//! optional components `grammachy engine` puts on disk and takes off again
//! (spec section 5.3), which is what makes LanguageTool an opt-in engine.

pub mod harper;
pub mod install;
pub mod languagetool;
pub mod local;
