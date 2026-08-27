//! What the server on the base URL actually serves, and the rule that decides
//! whether it is the model the Check asked for.
//!
//! HUF-236. llama-server ignores the `model` field of a chat request, so a
//! server left over from an earlier session answers every Check with the
//! weights it already holds. A benchmark row then prints one model name over
//! another model's numbers, and a Check reports a quality the chosen model
//! never produced. Nothing in the request prevents that, so the adapter asks
//! the server what it serves before the first Check.
//!
//! Two routes answer that question. `GET /v1/models` is the OpenAI one, and
//! llama-server answers it with a single entry. `GET /props` is the
//! llama-server extension, which names the weights file it loaded. The first
//! route that answers wins.
//!
//! A server that answers neither leaves the question open, and an open question
//! is not a mismatch: any OpenAI-compatible server may sit on the base URL, and
//! refusing one that keeps quiet would break a working install. The guard
//! therefore refuses a named mismatch and never a silence.

/// What one probe of the server found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Served {
    /// The server named the weights it holds.
    Id(String),
    /// Nothing listens on the port.
    Silent,
    /// The server answered, and did not say which weights it holds. That is
    /// every OpenAI-compatible server that is not llama-server, and it is also
    /// llama-server while it still reads the file, when it answers HTTP 503.
    Unknown,
}

/// The model id one `GET /v1/models` answer names.
///
/// llama-server answers with one entry whose `id` is the alias, which defaults
/// to the weights file it loaded.
pub fn from_models(raw: &serde_json::Value) -> Option<String> {
    let id = raw.get("data")?.get(0)?.get("id")?.as_str()?.trim();
    (!id.is_empty()).then(|| id.to_string())
}

/// The model id one `GET /props` answer names.
pub fn from_props(raw: &serde_json::Value) -> Option<String> {
    let path = raw.get("model_path")?.as_str()?.trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// Whether the weights a server named are the weights one model name asks for.
///
/// This is the rule [`super::unit::model_file`] resolves the `openaiModel`
/// setting on, applied to what the server reported rather than to a directory
/// listing: the name is a prefix of the weights file name, ignoring case. The
/// two must agree, because that setting is what picks the file a start loads.
/// A server names its weights as a path, a file name, or a bare alias, so only
/// the file name without its `.gguf` suffix is compared.
pub fn matches(served: &str, requested: &str) -> bool {
    let requested = requested.trim().to_ascii_lowercase();
    !requested.is_empty() && file_stem(served).starts_with(&requested)
}

/// The weights file name a server reported, without its directory or suffix.
pub fn file_stem(served: &str) -> String {
    let name = served
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(served)
        .to_ascii_lowercase();
    name.strip_suffix(".gguf").unwrap_or(&name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_model_list_of_llama_server_names_its_weights() {
        let raw = json!({
            "object": "list",
            "data": [{ "id": "qwen3.8-4b-Q4_K_M.gguf", "object": "model" }],
        });

        assert_eq!(from_models(&raw).as_deref(), Some("qwen3.8-4b-Q4_K_M.gguf"));
    }

    #[test]
    fn an_answer_that_names_no_model_is_no_answer() {
        for raw in [
            json!({}),
            json!({ "data": [] }),
            json!({ "data": [{ "object": "model" }] }),
            json!({ "data": [{ "id": "  " }] }),
            // A chat completion, which is what a stub that ignores the path
            // answers a probe with.
            json!({ "choices": [{ "message": { "content": "[]" } }] }),
        ] {
            assert_eq!(from_models(&raw), None, "{raw}");
        }
        assert_eq!(from_props(&json!({})), None);
        assert_eq!(from_props(&json!({ "model_path": "" })), None);
    }

    #[test]
    fn the_props_route_names_the_weights_file_it_loaded() {
        let raw =
            json!({ "model_path": "/home/a/.local/share/grammachy/models/qwen3.8-4b-Q4_K_M.gguf" });

        assert_eq!(
            from_props(&raw).as_deref(),
            Some("/home/a/.local/share/grammachy/models/qwen3.8-4b-Q4_K_M.gguf")
        );
    }

    /// The prefix rule of `unit::model_file`, so a setting name and the file a
    /// start loads for it agree here too.
    #[test]
    fn a_served_file_matches_the_name_its_download_was_asked_for() {
        for served in [
            "qwen3.8-4b-Q4_K_M.gguf",
            "/models/qwen3.8-4b-Q4_K_M.gguf",
            "QWEN3.8-4B-Q4_K_M.GGUF",
            "qwen3.8-4b",
        ] {
            assert!(matches(served, "qwen3.8-4b"), "{served}");
        }
    }

    #[test]
    fn another_model_never_passes_for_the_one_that_was_asked_for() {
        assert!(!matches("granite-4.2-3b-Q4_K_M.gguf", "gemma-4-e4b-it"));
        assert!(!matches("gemma-4-e4b-it-Q4_K_M.gguf", "qwen3.8-4b"));
        // A name that is only a piece of the requested one is not the weights
        // the Check asked for either.
        assert!(!matches("qwen3.8", "qwen3.8-4b"));
        assert!(!matches("qwen3.8-4b-Q4_K_M.gguf", "  "));
    }
}
