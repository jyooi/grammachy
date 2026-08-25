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
        chat_url: chat_url(&scheme, authority, path),
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

/// Where the chat completions live under one base URL.
///
/// A base URL that already ends in `/v1` only needs the rest of the path, which
/// is what a user pastes from another tool's documentation. Anything else gets
/// the whole OpenAI path.
fn chat_url(scheme: &str, authority: &str, path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let base = path.trim_end_matches('/');
    let suffix = if base.ends_with("/v1") {
        "/chat/completions"
    } else {
        "/v1/chat/completions"
    };
    format!("{scheme}://{authority}{base}{suffix}")
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

    #[test]
    fn a_missing_port_comes_from_the_scheme() {
        assert_eq!(parse("http://localhost").unwrap().port, 80);
        assert_eq!(parse("https://localhost").unwrap().port, 443);
    }
}
