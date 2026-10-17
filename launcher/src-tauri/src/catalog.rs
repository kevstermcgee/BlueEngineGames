use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::Duration};
use url::Url;
pub const FEED: &str = "https://kevstermcgee.github.io/BlueEngineGames/catalog.json";
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Package {
    pub kind: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub release: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Game {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub created: String,
    pub origin: String,
    pub presentation: String,
    pub networking: String,
    pub image: String,
    pub guide_url: String,
    pub wiki: serde_json::Value,
    pub package: Package,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Catalog {
    pub version: u32,
    pub games: Vec<Game>,
}
pub fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.split('-').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}
pub fn validate(c: &Catalog) -> Result<(), String> {
    if c.version != 1 || c.games.is_empty() || c.games.len() > 2000 {
        return Err("Unsupported game catalog".into());
    }
    let mut seen = HashSet::new();
    for g in &c.games {
        if !valid_slug(&g.slug) || !seen.insert(&g.slug) {
            return Err("Invalid or duplicate game identity".into());
        }
        let p = &g.package;
        let u = Url::parse(&p.url).map_err(|e| e.to_string())?;
        let path: Vec<_> = u.path().split('/').collect();
        if u.scheme() != "https"
            || u.host_str() != Some("github.com")
            || u.port().is_some()
            || !u.username().is_empty()
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
            || path.len() != 7
            || path[1] != "kevstermcgee"
            || !["BlueEngineGames", "Embertrail", "HarvestHollow"].contains(&path[2])
            || path[3] != "releases"
            || path[4] != "download"
            || path[5].is_empty()
        {
            return Err(format!("{} has an untrusted package URL", g.slug));
        }
        // GitHub release URLs have seven slash-separated fields (leading slash included).
        if !["zip", "installer"].contains(&p.kind.as_str())
            || p.sha256.len() != 64
            || !p.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || p.release.is_empty()
            || p.size == 0
            || p.size > 8 * 1024 * 1024 * 1024
        {
            return Err(format!("{} has invalid package metadata", g.slug));
        }
    }
    Ok(())
}
pub fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("BlueEngineGames/0.1.0")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(|e| e.to_string())
}
pub fn fetch() -> Result<Catalog, String> {
    use std::io::Read;
    let mut r = client()?
        .get(FEED)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    r.by_ref()
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 16 * 1024 * 1024 {
        return Err("Catalog is too large".into());
    }
    let c: Catalog = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    validate(&c)?;
    Ok(c)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slugs() {
        for s in ["../a", "a/b", "CON", "a--b", ""] {
            assert!(!valid_slug(s));
        }
        assert!(valid_slug("rift-delver"));
    }
}
