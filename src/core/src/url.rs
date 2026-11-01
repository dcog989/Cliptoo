//! URL helpers shared by the UI and the cache-maintenance code.

/// Host component of an absolute URL (e.g. "https://github.com/foo" ->
/// "github.com"). Returns `None` for relative or scheme-less input, which no
/// favicon or title is ever fetched for.
pub fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::extract_domain;

    #[test]
    fn extracts_host_from_http_urls() {
        assert_eq!(
            extract_domain("https://github.com/foo").unwrap(),
            "github.com"
        );
        assert_eq!(extract_domain("http://example.com").unwrap(), "example.com");
        assert_eq!(
            extract_domain("ftp://files.example.com/x").unwrap(),
            "files.example.com"
        );
    }

    #[test]
    fn normalizes_scheme_and_host_case() {
        assert_eq!(
            extract_domain("HTTP://EXAMPLE.COM/path").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn strips_userinfo_port_and_handles_ipv6() {
        assert_eq!(
            extract_domain("https://user:pass@example.com").unwrap(),
            "example.com"
        );
        assert_eq!(
            extract_domain("https://example.com:8443/x").unwrap(),
            "example.com"
        );
        assert_eq!(extract_domain("https://[::1]:8080/x").unwrap(), "[::1]");
    }

    #[test]
    fn rejects_urls_without_a_host() {
        for s in [
            "",
            "not a url",
            "www.example.com",
            "example.com",
            "/relative/path",
            "mailto:user@example.com",
        ] {
            assert_eq!(extract_domain(s), None, "for {s:?}");
        }
    }
}
