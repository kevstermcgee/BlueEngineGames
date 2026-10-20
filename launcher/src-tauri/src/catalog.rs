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
pub struct GameVersion {
    pub release: String,
    pub name: String,
    #[serde(default)]
    pub released_at: Option<String>,
    #[serde(default)]
    pub latest: bool,
    #[serde(default)]
    pub package: Option<Package>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Game {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub created: String,
    #[serde(default)]
    pub released_at: Option<String>,
    #[serde(default)]
    pub versions: Vec<GameVersion>,
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
fn validate_package(p: &Package, slug: &str) -> Result<(), String> {
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
        return Err(format!("{slug} has an untrusted package URL"));
    }
    // GitHub release URLs have seven slash-separated fields (leading slash included).
    if !["zip", "installer"].contains(&p.kind.as_str())
        || p.sha256.len() != 64
        || !p.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || p.release.is_empty()
        || p.size == 0
        || p.size > 8 * 1024 * 1024 * 1024
    {
        return Err(format!("{slug} has invalid package metadata"));
    }
    Ok(())
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
        validate_package(&g.package, &g.slug)?;
        let mut releases = HashSet::new();
        let mut latest = 0;
        for version in &g.versions {
            if version.release.is_empty() || !releases.insert(&version.release) {
                return Err(format!("{} has invalid release history", g.slug));
            }
            if version.latest {
                latest += 1;
                if version.release != g.package.release {
                    return Err(format!(
                        "{} has inconsistent latest release metadata",
                        g.slug
                    ));
                }
            }
            if let Some(package) = &version.package {
                validate_package(package, &g.slug)?;
                if package.release != version.release {
                    return Err(format!("{} has an inconsistent historical package", g.slug));
                }
            }
        }
        if !g.versions.is_empty() && latest != 1 {
            return Err(format!(
                "{} needs one latest release in its history",
                g.slug
            ));
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
    fn legacy_catalog() -> serde_json::Value {
        serde_json::json!({"version": 1, "games": [{
            "slug": "example", "name": "Example", "description": "A game", "version": "1.0",
            "created": "2026-01-01", "origin": "human", "presentation": "2d", "networking": "offline",
            "image": "", "guide_url": "", "wiki": {},
            "package": {"kind": "zip", "url": "https://github.com/kevstermcgee/BlueEngineGames/releases/download/current/example-windows-x64.zip",
                        "sha256": "a".repeat(64), "size": 1234, "release": "current"}
        }]})
    }
    #[test]
    fn slugs() {
        for s in ["../a", "a/b", "CON", "a--b", ""] {
            assert!(!valid_slug(s));
        }
        assert!(valid_slug("rift-delver"));
    }
    #[test]
    fn old_catalogs_keep_release_dates_unknown_and_still_load() {
        let c: Catalog = serde_json::from_value(legacy_catalog()).unwrap();
        validate(&c).unwrap();
        assert!(c.games[0].released_at.is_none());
        assert!(c.games[0].versions.is_empty());
    }
    #[test]
    fn published_version_dates_and_unavailable_historical_packages_round_trip() {
        let mut data = legacy_catalog();
        data["games"][0]["released_at"] = "2026-10-18T08:00:00Z".into();
        data["games"][0]["versions"] = serde_json::json!([
            {"release": "current", "name": "Example 1.0", "released_at": "2026-10-18T08:00:00Z",
             "latest": true, "package": data["games"][0]["package"].clone()},
            {"release": "old", "name": "Example preview", "released_at": null,
             "latest": false, "package": null}
        ]);
        let c: Catalog = serde_json::from_value(data.clone()).unwrap();
        validate(&c).unwrap();
        assert_eq!(serde_json::to_value(c).unwrap(), data);
    }
    #[test]
    fn historical_packages_have_the_same_url_trust_checks_as_latest() {
        let mut data = legacy_catalog();
        let mut old = data["games"][0]["package"].clone();
        old["release"] = "old".into();
        old["url"] = "https://untrusted.example/game.zip".into();
        data["games"][0]["versions"] = serde_json::json!([
            {"release": "current", "name": "Current", "latest": true},
            {"release": "old", "name": "Old", "package": old}
        ]);
        let c: Catalog = serde_json::from_value(data).unwrap();
        assert!(validate(&c).unwrap_err().contains("untrusted package URL"));
    }
}
