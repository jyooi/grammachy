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
//! llama-server extension, which names the weights file it loaded. An answer
//! that matches the requested model wins, whichever route gave it, because a
//! `--alias` renames what `/v1/models` reports and leaves `/props` truthful.
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

/// The model id one `GET /v1/models` answer names, for one requested name.
///
/// llama-server answers with one entry whose `id` is the alias, which defaults
/// to the weights file it loaded. Ollama and LM Studio instead list every model
/// they can serve, in an order this adapter does not decide. So the whole list
/// is read and an entry that matches the requested name wins. A list that holds
/// no such entry answers with its first named entry, which is what the refusal
/// then names.
pub fn from_models(raw: &serde_json::Value, requested: &str) -> Option<String> {
    let ids: Vec<&str> = raw
        .get("data")?
        .as_array()?
        .iter()
        .filter_map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect();

    ids.iter()
        .find(|id| matches(id, requested))
        .or(ids.first())
        .map(|id| id.to_string())
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
/// A server names its weights as a path, a file name, or a bare alias, so both
/// sides are cut down the same way, to the file name without its `.gguf`
/// suffix. `model_file` resolves a setting that already carries that suffix, so
/// the guard must not then call the very file such a start loaded a mismatch.
pub fn matches(served: &str, requested: &str) -> bool {
    let requested = file_stem(requested);
    !requested.is_empty() && file_stem(served).starts_with(&requested)
}

/// The weights file name a server reported, without its directory.
///
/// llama.cpp names its weights as the `--model` path it was started with, and
/// that path holds the home directory of whoever runs it. One bench run is the
/// whole committed benchmark file, so nothing outside this module may carry the
/// directory. The file name keeps the quantisation, which is what a reader of
/// that file needs.
pub fn file_name(served: &str) -> &str {
    let served = served.trim();
    served.rsplit(['/', '\\']).next().unwrap_or(served)
}

/// The weights file name a server reported, without its directory or suffix.
pub fn file_stem(served: &str) -> String {
    let name = file_name(served).to_ascii_lowercase();
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

        assert_eq!(
            from_models(&raw, "qwen3.8-4b").as_deref(),
            Some("qwen3.8-4b-Q4_K_M.gguf")
        );
    }

    /// Ollama and LM Studio list every model they can serve, and the order is
    /// not the client's to pick. The requested one wins wherever it sits.
    #[test]
    fn a_list_of_many_models_answers_with_the_one_that_was_asked_for() {
        let raw = json!({
            "object": "list",
            "data": [
                { "id": "gemma3:latest", "object": "model" },
                { "id": "qwen3.8-4b:latest", "object": "model" },
                { "id": "granite-4.2-3b", "object": "model" },
            ],
        });

        assert_eq!(
            from_models(&raw, "qwen3.8-4b").as_deref(),
            Some("qwen3.8-4b:latest")
        );
        // A list that holds none of them names its first entry, so the refusal
        // can say what the server does hold.
        assert_eq!(
            from_models(&raw, "phi-5-mini").as_deref(),
            Some("gemma3:latest")
        );
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
            assert_eq!(from_models(&raw, "qwen3.8-4b"), None, "{raw}");
        }
        assert_eq!(from_props(&json!({})), None);
        assert_eq!(from_props(&json!({ "model_path": "" })), None);
    }

    /// The `--model` path of a llama.cpp unit holds a home directory, and one
    /// bench run is a committed file, so only the file name may leave here.
    #[test]
    fn a_served_path_is_cut_down_to_the_weights_file_name() {
        for served in [
            "/home/someone/.local/share/grammachy/models/gemma-4-E4B-it-Q4_K_M.gguf",
            "  /home/someone/.local/share/grammachy/models/gemma-4-E4B-it-Q4_K_M.gguf  ",
            "gemma-4-E4B-it-Q4_K_M.gguf",
        ] {
            assert_eq!(file_name(served), "gemma-4-E4B-it-Q4_K_M.gguf", "{served}");
        }
        assert_eq!(file_name("local-llm"), "local-llm");
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

    /// `model_file` resolves a setting that already carries the suffix, so a
    /// start loads that exact file. The guard must agree with it.
    #[test]
    fn a_requested_name_that_carries_the_gguf_suffix_matches_the_file_it_names() {
        for served in [
            "gemma-4-e4b-it-Q4_K_M.gguf",
            "/models/gemma-4-e4b-it-Q4_K_M.gguf",
            "gemma-4-e4b-it-q4_k_m",
        ] {
            assert!(matches(served, "gemma-4-e4b-it-Q4_K_M.gguf"), "{served}");
        }
        assert!(!matches(
            "granite-4.2-3b-Q4_K_M.gguf",
            "gemma-4-e4b-it-Q4_K_M.gguf"
        ));
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
