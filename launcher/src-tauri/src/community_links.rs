//! Only public source, checks and native release links leave the local launcher UI.
use url::Url;

pub fn validate(value: &str) -> Result<(), String> {
    let denied = || "This collaboration link is not trusted.".to_string();
    if value.chars().any(char::is_control) || value.contains(['%', '\\']) {
        return Err(denied());
    }
    let url = Url::parse(value).map_err(|_| denied())?;
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(denied());
    }
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    let commit = |part: &str| part.len() == 40 && part.bytes().all(|b| b.is_ascii_hexdigit());
    let valid = match url.host_str() {
        Some("github.com") if parts.len() >= 4 => match parts[2] {
            "tree" => commit(parts[3]),
            "archive" => parts.len() == 4 && parts[3].strip_suffix(".tar.gz").is_some_and(commit),
            "commits" => {
                parts.len() == 6
                    && parts[3] == "main"
                    && parts[4] == "games"
                    && crate::catalog::valid_slug(parts[5])
            }
            "pull" => parts.len() == 4 && parts[3].bytes().all(|b| b.is_ascii_digit()),
            "blob" => {
                parts[0] == "kevstermcgee"
                    && parts[1] == "BlueEngine"
                    && parts[3] == "main"
                    && (parts[4..] == ["AGENTS.md"] || parts[4..] == ["docs", "COLLABORATION.md"])
            }
            "releases" => {
                parts[0] == "kevstermcgee"
                    && parts[1] == "BlueEngineGames"
                    && ((parts.len() == 5 && parts[3] == "tag")
                        || (parts.len() == 6
                            && parts[3] == "download"
                            && parts[5].ends_with("-setup-windows-x64.exe")))
            }
            _ => false,
        },
        Some("codeload.github.com") => parts.len() == 4 && parts[2] == "tar.gz" && commit(parts[3]),
        Some("kevstermcgee.github.io") => {
            parts.len() == 3
                && parts[0] == "BlueEngineGames"
                && parts[1] == "games"
                && crate::catalog::valid_slug(parts[2])
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(denied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_public_collaboration_links_open_in_the_browser() {
        let sha = "a".repeat(40);
        for value in [format!("https://github.com/cousin/BlueEngine/tree/{sha}/games/remix"),
            format!("https://github.com/cousin/BlueEngine/archive/{sha}.tar.gz"),
            "https://github.com/kevstermcgee/BlueEngine/pull/42".into(),
            "https://github.com/kevstermcgee/BlueEngine/commits/main/games/wormhole".into(),
            "https://github.com/kevstermcgee/BlueEngine/blob/main/docs/COLLABORATION.md".into(),
            "https://github.com/kevstermcgee/BlueEngineGames/releases/download/games-abc/remix-setup-windows-x64.exe".into()] {
            assert!(validate(&value).is_ok(), "{value}");
        }
        for value in [
            "file:///C:/Windows/cmd.exe",
            "http://github.com/owner/repo/pull/42",
            "https://evil.example/game",
            "https://github.com@evil.example/owner/repo/pull/42",
            "https://github.com/owner/repo/tree/main",
            "https://github.com/owner/repo/pull/42?token=secret",
            "https://github.com/owner/repo/releases/download/tag/payload.exe",
            "https://github.com/owner/repo/pull/%34%32",
            "https://github.com/owner/repo/pull/42\n",
        ] {
            assert!(validate(value).is_err(), "{value}");
        }
    }
}
