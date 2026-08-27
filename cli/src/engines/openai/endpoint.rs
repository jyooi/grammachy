//! The base URL of the `openai` engine, and the loopback rule it must obey.
//!
//! Spec section 4: the host must be `localhost`, `127.0.0.1`, or `::1` in v1.
//! Any other host is `bad_arguments`, never a request. That rule is a product
//! guarantee, not a convenience: the Selection is whatever the user highlighted
//! anywhere on the machine, so it never leaves the machine.
//!
//! The parser here is deliberate rather than general. It accepts the shape a
//! Settings text field realistically holds, rejects everything it does not
//! understand, and never guesses. Userinfo is rejected outright, because
//! `http://127.0.0.1@evil.example/` is a remote host wearing a loopback mask.

/// The three hosts spec section 4 allows.
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// One accepted base URL, taken apart into the pieces the adapter needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The host as it was written, lowercased and without brackets.
    pub host: String,
    pub port: u16,
    /// The full chat completion URL the adapter posts to.
    pub chat_url: String,
    /// The OpenAI model list of this server, which names the weights it holds.
    pub models_url: String,
    /// The llama.cpp `/props` endpoint, the second way to ask the same
    /// question. It sits at the root of the server and not under `/v1`.
    pub props_url: String,
}

impl Endpoint {
    /// The address `llama-server` binds, which is the host as an IP.
    pub fn bind_host(&self) -> &str {
        match self.host.as_str() {
            "localhost" => "127.0.0.1",
            other => other,
        }
    }

    /// Host and port as one string, the way error messages name a server.
    pub fn address(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// Take one base URL apart, or answer why it is not usable.
///
/// The message is what the `bad_arguments` envelope carries, so it says what is
/// wrong in one sentence a user can act on.
pub fn parse(base_url: &str) -> Result<Endpoint, String> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err("The OpenAI base URL is empty.".to_string());
    }

    let (scheme, rest) = trimmed
        .split_once("://")
        .ok_or_else(|| format!("The OpenAI base URL has no scheme: {trimmed}"))?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "The OpenAI base URL must be http or https, not {scheme}."
        ));
    }

    let path_start = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(path_start);
    if authority.contains('@') {
        return Err(format!(
            "The OpenAI base URL carries a user name, which v1 does not accept: {trimmed}"
        ));
    }

    let (host, port) = split_host_and_port(authority, &scheme)?;
    if !LOOPBACK_HOSTS.contains(&host.as_str()) {
        return Err(format!(
            "The OpenAI base URL must stay on this machine. Its host is {host}, and v1 accepts only localhost, 127.0.0.1, and ::1."
        ));
    }

    Ok(Endpoint {
        host,
        port,
        chat_url: api_url(&scheme, authority, path, "/chat/completions"),
        models_url: api_url(&scheme, authority, path, "/models"),
        props_url: props_url(&scheme, authority, path),
    })
}

/// The host without brackets and the port, defaulted from the scheme.
fn split_host_and_port(authority: &str, scheme: &str) -> Result<(String, u16), String> {
    let default_port = if scheme == "https" { 443 } else { 80 };

    // An IPv6 host is bracketed, so the colons inside it are not the port.
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest
            .split_once(']')
            .ok_or_else(|| format!("The OpenAI base URL has an unclosed bracket: {authority}"))?;
        let port = match after.strip_prefix(':') {
            Some(digits) => parse_port(digits)?,
            None if after.is_empty() => default_port,
            None => return Err(format!("The OpenAI base URL is not a URL: {authority}")),
        };
        return Ok((host.to_ascii_lowercase(), port));
    }

    match authority.rsplit_once(':') {
        Some((host, digits)) => Ok((host.to_ascii_lowercase(), parse_port(digits)?)),
        None => Ok((authority.to_ascii_lowercase(), default_port)),
    }
}

fn parse_port(digits: &str) -> Result<u16, String> {
    digits
        .parse()
        .map_err(|_| format!("The OpenAI base URL has no usable port: {digits}"))
}

/// Where one OpenAI route lives under one base URL.
///
/// A base URL that already ends in `/v1` only needs the rest of the path, which
/// is what a user pastes from another tool's documentation. Anything else gets
/// the whole OpenAI path.
fn api_url(scheme: &str, authority: &str, path: &str, suffix: &str) -> String {
    let base = trimmed_path(path);
    let version = if base.ends_with("/v1") { "" } else { "/v1" };
    format!("{scheme}://{authority}{base}{version}{suffix}")
}

/// Where the llama.cpp `/props` endpoint lives under one base URL.
///
/// That route is a llama.cpp extension rather than an OpenAI one, so it sits at
/// the root of the server. A base URL that names `/v1` therefore loses it here.
fn props_url(scheme: &str, authority: &str, path: &str) -> String {
    let base = trimmed_path(path);
    let root = base.strip_suffix("/v1").unwrap_or(base);
    format!("{scheme}://{authority}{root}/props")
}

/// The path of one base URL, without its query, fragment, or trailing slash.
fn trimmed_path(path: &str) -> &str {
    path.split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_base_url_is_the_llama_cpp_server() {
        let endpoint = parse(crate::settings::DEFAULT_OPENAI_BASE_URL).expect("the default parses");

        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 8080);
        assert_eq!(
            endpoint.chat_url,
            "http://127.0.0.1:8080/v1/chat/completions"
        );
        assert_eq!(endpoint.bind_host(), "127.0.0.1");
    }

    #[test]
    fn every_loopback_host_the_spec_names_is_accepted() {
        assert_eq!(parse("http://localhost:8080").unwrap().host, "localhost");
        assert_eq!(parse("http://LOCALHOST:8080").unwrap().host, "localhost");
        assert_eq!(parse("http://127.0.0.1:8080").unwrap().host, "127.0.0.1");

        let six = parse("http://[::1]:8080").expect("the bracketed IPv6 host parses");
        assert_eq!(six.host, "::1");
        assert_eq!(six.port, 8080);
        assert_eq!(six.chat_url, "http://[::1]:8080/v1/chat/completions");
        assert_eq!(six.address(), "[::1]:8080");
    }

    #[test]
    fn a_host_that_is_not_loopback_is_refused() {
        for base_url in [
            "http://example.com:8080",
            "https://api.openai.com/v1",
            "http://192.168.1.10:8080",
            "http://127.0.0.2:8080",
            "http://localhost.evil.example:8080",
            "http://[::2]:8080",
        ] {
            let message = parse(base_url).expect_err("a remote host is refused");
            assert!(
                message.contains("only localhost"),
                "{base_url} names the rule: {message}"
            );
        }
    }

    #[test]
    fn userinfo_never_masks_a_remote_host() {
        let message = parse("http://127.0.0.1@evil.example/v1").expect_err("userinfo is refused");

        assert!(message.contains("user name"), "{message}");
    }

    #[test]
    fn a_scheme_that_is_not_http_is_refused() {
        assert!(parse("127.0.0.1:8080").is_err());
        assert!(parse("ftp://127.0.0.1:8080").is_err());
        assert!(parse("file:///etc/passwd").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn a_base_url_that_already_ends_in_v1_is_not_doubled() {
        assert_eq!(
            parse("http://127.0.0.1:8080/v1").unwrap().chat_url,
            "http://127.0.0.1:8080/v1/chat/completions"
        );
        assert_eq!(
            parse("http://127.0.0.1:8080/v1/").unwrap().chat_url,
            "http://127.0.0.1:8080/v1/chat/completions"
        );
        assert_eq!(
            parse("http://127.0.0.1:8080/").unwrap().chat_url,
            "http://127.0.0.1:8080/v1/chat/completions"
        );
        // A proxy that mounts the API under a prefix still works.
        assert_eq!(
            parse("http://localhost:9000/llm").unwrap().chat_url,
            "http://localhost:9000/llm/v1/chat/completions"
        );
    }

    /// The served-model guard of HUF-236 asks these two routes, so both have to
    /// land where llama-server actually serves them: the model list under the
    /// OpenAI version prefix, and `/props` at the root beside it.
    #[test]
    fn the_served_model_routes_sit_beside_the_chat_completions() {
        let plain = parse("http://127.0.0.1:8080").expect("the base URL parses");
        assert_eq!(plain.models_url, "http://127.0.0.1:8080/v1/models");
        assert_eq!(plain.props_url, "http://127.0.0.1:8080/props");

        let versioned = parse("http://127.0.0.1:8080/v1/").expect("the base URL parses");
        assert_eq!(versioned.models_url, "http://127.0.0.1:8080/v1/models");
        assert_eq!(versioned.props_url, "http://127.0.0.1:8080/props");

        // A proxy that mounts the API under a prefix keeps that prefix on both.
        let mounted = parse("http://localhost:9000/llm").expect("the base URL parses");
        assert_eq!(mounted.models_url, "http://localhost:9000/llm/v1/models");
        assert_eq!(mounted.props_url, "http://localhost:9000/llm/props");
    }

    #[test]
    fn a_missing_port_comes_from_the_scheme() {
        assert_eq!(parse("http://localhost").unwrap().port, 80);
        assert_eq!(parse("https://localhost").unwrap().port, 443);
    }
}
