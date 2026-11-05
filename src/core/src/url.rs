//! URL helpers shared by the UI and the cache-maintenance code.

use crate::image::HASH_FILENAME_PREFIX_LEN;

/// Host component of an absolute URL (e.g. "https://github.com/foo" ->
/// "github.com"). Returns `None` for relative or scheme-less input, which no
/// favicon or title is ever fetched for.
pub fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(ToOwned::to_owned)
}

/// Cache key for per-URL cached data (favicon, page title): the first
/// [`HASH_FILENAME_PREFIX_LEN`] chars of the SHA-256 of the URL.
///
/// Keyed by the page, not the host: unrelated pages share a host
/// (`dcog989.github.io/Cliptoo` and `/ProtonVariants`) and nothing in the URL
/// marks where one site ends and the next begins, so a host keyed cache lets
/// the first page fetched stand in for every page on that host.
pub fn url_cache_key(url: &str) -> Option<String> {
    extract_domain(url)?;
    let (hash, _) = crate::content::hash::sha256_hex_and_prefix(url);
    Some(hash[..HASH_FILENAME_PREFIX_LEN].to_owned())
}

#[cfg(test)]
mod tests {
    use super::{extract_domain, url_cache_key};
    use crate::image::HASH_FILENAME_PREFIX_LEN;

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

    #[test]
    fn url_key_is_per_page_not_per_host() {
        let cliptoo = url_cache_key("https://dcog989.github.io/Cliptoo/").unwrap();
        let proton = url_cache_key("https://dcog989.github.io/ProtonVariants/").unwrap();
        assert_ne!(cliptoo, proton, "two pages on one host share no key");
        assert_eq!(
            url_cache_key("https://dcog989.github.io/Cliptoo/").unwrap(),
            cliptoo,
            "the same page always maps to the same key"
        );
        assert_eq!(cliptoo.len(), HASH_FILENAME_PREFIX_LEN);
        assert!(cliptoo.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn url_key_rejects_non_absolute_urls() {
        for s in ["", "www.example.com", "/relative/path", "mailto:a@b.c"] {
            assert_eq!(url_cache_key(s), None, "for {s:?}");
        }
    }
}
