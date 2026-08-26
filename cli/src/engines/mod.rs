//! One module per engine adapter, plugged into the seam in [`crate::engine`].

pub mod harper;
pub mod languagetool;
pub mod local;
pub mod openai;
pub mod openrouter;
