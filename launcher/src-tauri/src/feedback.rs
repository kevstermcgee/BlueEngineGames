use url::Url;

/// Only the public feedback repository may leave the bundled WebView.
pub fn validate_url(value: &str) -> Result<(), String> {
    if value.len() > 7500 || value.chars().any(char::is_control) {
        return Err("This feedback link is too long or invalid".into());
    }
    let url = Url::parse(value).map_err(|_| "Invalid feedback link")?;
    let root = "/kevstermcgee/BlueEngine/issues";
    let path = url.path();
    let issue = path
        .strip_prefix(&format!("{root}/"))
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()));
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
        || !(path == root || path == format!("{root}/new") || issue)
    {
        return Err("Only BlueEngine feedback links can be opened".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permits_feedback_handoffs_and_public_review_links() {
        for path in [
            "",
            "/new?template=player-feedback.yml&enjoyed=%26%22%F0%9F%8E%AE",
            "?q=is%3Aissue+%22Player+feedback%22+riftwake",
            "/123",
        ] {
            assert!(validate_url(&format!(
                "https://github.com/kevstermcgee/BlueEngine/issues{path}"
            ))
            .is_ok());
        }
    }

    #[test]
    fn rejects_other_destinations_and_unsafe_schemes() {
        for url in [
            "javascript:alert(1)",
            "file:///C:/Windows/System32/cmd.exe",
            "https://example.com/kevstermcgee/BlueEngine/issues/new",
            "https://github.com.evil.example/kevstermcgee/BlueEngine/issues/new",
            "https://github.com@evil.example/kevstermcgee/BlueEngine/issues/new",
            "https://user:pass@github.com/kevstermcgee/BlueEngine/issues/new",
            "http://github.com/kevstermcgee/BlueEngine/issues/new",
            "https://github.com:444/kevstermcgee/BlueEngine/issues/new",
            "https://github.com/kevstermcgee/Other/issues/new",
            "https://github.com/kevstermcgee/BlueEngine/issues/new/../../settings",
            "https://github.com/kevstermcgee/BlueEngine/issues/new#anything",
            "https://github.com/kevstermcgee/BlueEngine/issues/new\n",
        ] {
            assert!(validate_url(url).is_err(), "accepted {url}");
        }
        assert!(validate_url(&format!(
            "https://github.com/kevstermcgee/BlueEngine/issues/new?{}",
            "x".repeat(7500)
        ))
        .is_err());
    }
}
