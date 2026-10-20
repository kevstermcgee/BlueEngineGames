use crate::catalog::{client, Game};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
const METADATA_RECEIPT: &str = ".installed-metadata.json";
const METADATA_PENDING: &str = ".installed-metadata.pending";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct InstallationMetadata {
    schema_version: u32,
    pub release: String,
    pub version: String,
    pub released_at: Option<String>,
    /// Local completion time for this installed release, in Unix milliseconds.
    pub installed_at: u64,
}

/// Legacy installs have no historical local timestamp. Reading a library never
/// invents one from a scan, current time, or filesystem modification time.
pub fn metadata(dir: &Path, release: &str) -> Option<InstallationMetadata> {
    let mut data = Vec::new();
    File::open(dir.join(METADATA_RECEIPT))
        .ok()?
        .take(8193)
        .read_to_end(&mut data)
        .ok()?;
    if data.len() > 8192 {
        return None;
    }
    let receipt: InstallationMetadata = serde_json::from_slice(&data).ok()?;
    if receipt.schema_version != 1
        || release.is_empty()
        || receipt.release != release
        || receipt.version.trim().is_empty()
        || receipt.version.len() > 1024
        || receipt.installed_at == 0
        || receipt.installed_at > 253_402_300_799_999
        || receipt
            .released_at
            .as_ref()
            .is_some_and(|date| date.len() > 160)
    {
        return None;
    }
    Some(receipt)
}

fn write_metadata(game: &Game, target: &Path) -> Result<(), String> {
    let installed_at = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "The system clock is before the Unix epoch")?
            .as_millis(),
    )
    .map_err(|_| "The system clock is outside the supported range")?;
    let receipt = InstallationMetadata {
        schema_version: 1,
        release: game.package.release.clone(),
        version: if game.version.trim().is_empty() {
            game.package.release.clone()
        } else {
            game.version.clone()
        },
        released_at: game.released_at.clone(),
        installed_at,
    };
    let data = serde_json::to_vec(&receipt).map_err(|error| error.to_string())?;
    let temporary = target.join(METADATA_PENDING);
    let result = (|| {
        let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&data).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        // Same-directory rename replaces only after the complete receipt is
        // durable, preserving an earlier receipt on a failed write.
        fs::rename(&temporary, target.join(METADATA_RECEIPT)).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn launcher_receipt(name: &str) -> bool {
    [
        ".installed-files.tsv",
        ".installed-release",
        METADATA_RECEIPT,
        METADATA_PENDING,
    ]
    .iter()
    .any(|receipt| name.eq_ignore_ascii_case(receipt))
}
pub fn digest(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut b = [0; 65536];
    loop {
        let n = f.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn safe_relative(s: &str) -> Result<PathBuf, String> {
    if s.is_empty() || s.contains(['\\', ':', '\0']) || s.starts_with('/') {
        return Err("Unsafe package path".into());
    }
    let parts: Vec<_> = s.trim_end_matches('/').split('/').collect();
    for part in &parts {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty()
            || *part == "."
            || *part == ".."
            || part.ends_with([' ', '.'])
            || part.chars().any(|c| c < ' ' || "<>\"|?*".contains(c))
            || [
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9",
            ]
            .contains(&stem.as_str())
        {
            return Err("Unsafe Windows package path".into());
        }
    }
    if parts.iter().any(|s| launcher_receipt(s)) {
        return Err("Package overwrites launcher receipts".into());
    }
    Ok(parts.iter().collect())
}
pub fn extract(zip: &Path, dest: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut archive = zip::ZipArchive::new(File::open(zip).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if archive.len() > 100000 {
        return Err("Too many package entries".into());
    }
    let mut seen = HashSet::new();
    let mut total = 0u64;
    let mut files = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let rel = safe_relative(&name)?;
        if !seen.insert(name.trim_end_matches('/').to_ascii_lowercase()) {
            return Err("Duplicate package entry".into());
        }
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("Package contains a symbolic link".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("Package size overflow")?;
        if total > 24 * 1024 * 1024 * 1024 {
            return Err("Expanded package is too large".into());
        }
        let target = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(target).map_err(|e| e.to_string())?;
            continue;
        }
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut out = File::create(&target).map_err(|e| e.to_string())?;
        let expected = entry.size();
        let written = std::io::copy(&mut entry.by_ref().take(expected + 1), &mut out)
            .map_err(|e| e.to_string())?;
        if written != expected {
            return Err("Expanded package entry has an invalid size".into());
        }
        out.sync_all().map_err(|e| e.to_string())?;
        files.insert(name, digest(&target)?);
    }
    Ok(files)
}
pub fn playable(dir: &Path, slug: &str) -> Option<PathBuf> {
    for n in [
        format!("Play-{slug}.exe"),
        format!("{slug}.exe"),
        format!("{}.exe", slug.replace('-', "_")),
        format!("{}.exe", slug.replace('-', "")),
    ] {
        let p = dir.join(n);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut candidates = Vec::new();
    fn visit(d: &Path, depth: u8, out: &mut Vec<PathBuf>) {
        if depth > 3 {
            return;
        }
        if let Ok(entries) = fs::read_dir(d) {
            for e in entries.flatten() {
                let p = e.path();
                if e.file_type().is_ok_and(|t| t.is_symlink()) {
                    continue;
                }
                if p.is_dir() {
                    visit(&p, depth + 1, out)
                } else if p.extension().is_some_and(|s| s.eq_ignore_ascii_case("exe")) {
                    let n = p.file_name().unwrap().to_string_lossy().to_lowercase();
                    if !n.starts_with("update-")
                        && !n.starts_with("unins")
                        && !n.contains("server")
                        && !n.contains("crash")
                        && !n.contains("installer")
                    {
                        out.push(p)
                    }
                }
            }
        }
    }
    visit(dir, 0, &mut candidates);
    candidates.sort();
    if candidates.len() == 1 {
        candidates.pop()
    } else {
        candidates.into_iter().find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .eq_ignore_ascii_case("BlueEngine.exe")
        })
    }
}
fn receipt(dir: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(dir.join(".installed-files.tsv"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            line.split_once('\t')
                .map(|(a, b)| (a.to_string(), b.to_string()))
        })
        .collect()
}
pub fn preserve(old: &Path, new: &Path, shipped: &BTreeMap<String, String>) -> Result<(), String> {
    let previous = receipt(old);
    fn walk(
        base: &Path,
        d: &Path,
        new: &Path,
        shipped: &BTreeMap<String, String>,
        previous: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        for entry in fs::read_dir(d).map_err(|e| e.to_string())? {
            let e = entry.map_err(|e| e.to_string())?;
            let p = e.path();
            let t = e.file_type().map_err(|e| e.to_string())?;
            if t.is_symlink() {
                return Err(
                    "Existing installation contains a symbolic link; move it before updating"
                        .into(),
                );
            }
            let rel = p.strip_prefix(base).unwrap();
            let key = rel.to_string_lossy().replace('\\', "/");
            if t.is_dir() {
                walk(base, &p, new, shipped, previous)?;
            } else if !launcher_receipt(&key)
                && !shipped.keys().any(|k| k.eq_ignore_ascii_case(&key))
                && !previous
                    .get(&key)
                    .is_some_and(|h| digest(&p).is_ok_and(|v| v.eq_ignore_ascii_case(h)))
            {
                let target = new.join(rel);
                fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                fs::copy(p, target).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    if old.exists() {
        walk(old, old, new, shipped, &previous)?;
    }
    Ok(())
}
pub fn download(
    g: &Game,
    path: &Path,
    cancel: &AtomicBool,
    progress: impl Fn(u64, u64),
) -> Result<(), String> {
    let mut r = client()?
        .get(&g.package.url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut f = File::create(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut received = 0;
    let mut last_progress = std::time::Instant::now();
    let mut b = [0; 65536];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Download cancelled".into());
        }
        let n = r.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        received += n as u64;
        if received > g.package.size {
            return Err("Download exceeds published size".into());
        }
        h.update(&b[..n]);
        f.write_all(&b[..n]).map_err(|e| e.to_string())?;
        if last_progress.elapsed().as_millis() >= 150 || received == g.package.size {
            progress(received, g.package.size);
            last_progress = std::time::Instant::now();
        }
    }
    if received != g.package.size
        || format!("{:x}", h.finalize()) != g.package.sha256.to_lowercase()
    {
        return Err("Download verification failed. Existing installation was kept.".into());
    }
    f.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}
pub fn install_zip(g: &Game, zip: &Path, target: &Path) -> Result<(), String> {
    let parent = target.parent().ok_or("Missing install parent")?;
    let stage = parent.join(format!(".{}-staging", g.slug));
    let backup = parent.join(format!(".{}-backup", g.slug));
    if backup.exists() {
        if !target.exists() {
            fs::rename(&backup, target).map_err(|e| e.to_string())?;
        } else {
            fs::remove_dir_all(&backup).map_err(|e| e.to_string())?;
        }
    }
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let prepare = (|| {
        let files = extract(zip, &stage)?;
        if playable(&stage, &g.slug).is_none() {
            return Err("Package contains no recognized playable executable".into());
        }
        preserve(target, &stage, &files)?;
        let manifest = files
            .iter()
            .map(|(p, h)| format!("{p}\t{h}\n"))
            .collect::<String>();
        fs::write(stage.join(".installed-files.tsv"), manifest).map_err(|e| e.to_string())?;
        fs::write(stage.join(".installed-release"), &g.package.release)
            .map_err(|e| e.to_string())?;
        write_metadata(g, &stage)?;
        Ok::<_, String>(())
    })();
    if let Err(e) = prepare {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    if target.exists() {
        fs::rename(target, &backup).map_err(|e| format!("Close the game before updating: {e}"))?;
    }
    if let Err(e) = fs::rename(&stage, target) {
        if backup.exists() {
            let _ = fs::rename(&backup, target);
        }
        return Err(e.to_string());
    }
    let _ = fs::remove_dir_all(backup);
    Ok(())
}
pub fn uninstall(dir: &Path) -> Result<(), String> {
    let files = receipt(dir);
    if files.is_empty() {
        return Err("This installation has no file receipt. Use its Windows uninstaller to preserve your saves.".into());
    }
    for (name, hash) in files {
        if let Ok(rel) = safe_relative(&name) {
            let p = dir.join(rel);
            if p.is_file() && digest(&p).is_ok_and(|h| h.eq_ignore_ascii_case(&hash)) {
                fs::remove_file(p).map_err(|e| e.to_string())?;
            }
        }
    }
    let _ = fs::remove_file(dir.join(".installed-release"));
    let _ = fs::remove_file(dir.join(".installed-files.tsv"));
    let _ = fs::remove_file(dir.join(METADATA_RECEIPT));
    let _ = fs::remove_file(dir.join(METADATA_PENDING));
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_paths() {
        for p in [
            "../x",
            "a/../../x",
            "/a",
            "a\\b",
            "x:stream",
            "CON.txt",
            "a/NUL",
            "a/",
            "a/./b",
            "a. /x",
        ] {
            if p == "a/" {
                assert!(safe_relative(p).is_ok());
            } else {
                assert!(safe_relative(p).is_err(), "{p}");
            }
        }
    }
    #[test]
    fn preserves_saves_and_modified_obsolete() {
        let t = tempfile::tempdir().unwrap();
        let old = t.path().join("old");
        let new = t.path().join("new");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("save.json"), "save").unwrap();
        fs::write(old.join("old.dll"), "old").unwrap();
        fs::write(old.join("edited.txt"), "custom").unwrap();
        fs::write(old.join(METADATA_RECEIPT), "old version dates").unwrap();
        fs::write(
            old.join(".installed-files.tsv"),
            format!(
                "old.dll\t{}\nedited.txt\t{}\n",
                digest(&old.join("old.dll")).unwrap(),
                "0".repeat(64)
            ),
        )
        .unwrap();
        preserve(&old, &new, &BTreeMap::new()).unwrap();
        assert!(new.join("save.json").exists());
        assert!(new.join("edited.txt").exists());
        assert!(!new.join("old.dll").exists());
        assert!(!new.join(METADATA_RECEIPT).exists());
    }
}

#[cfg(test)]
mod transaction_tests {
    use super::*;
    use crate::catalog::Package;
    use zip::write::SimpleFileOptions;
    fn package(path: &Path, entries: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, data) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    fn game() -> Game {
        Game {
            slug: "test-game".into(),
            name: "Test".into(),
            description: "".into(),
            version: "1".into(),
            released_at: None,
            versions: Vec::new(),
            created: "".into(),
            origin: "human".into(),
            presentation: "2d".into(),
            networking: "offline".into(),
            image: "".into(),
            guide_url: "".into(),
            wiki: serde_json::json!({}),
            package: Package {
                kind: "zip".into(),
                url: "".into(),
                sha256: "".into(),
                size: 1,
                release: "new-build".into(),
            },
        }
    }
    #[test]
    fn bad_package_never_changes_existing_installation() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("test-game");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("test-game.exe"), "working").unwrap();
        let zip = t.path().join("bad.zip");
        package(&zip, &[("../escaped.exe", "bad")]);
        assert!(install_zip(&game(), &zip, &dir).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("test-game.exe")).unwrap(),
            "working"
        );
        assert!(!t.path().join("escaped.exe").exists());
    }
    #[test]
    fn zip_case_collisions_and_reserved_receipts_are_rejected() {
        let t = tempfile::tempdir().unwrap();
        for entries in [
            vec![("Game.exe", "a"), ("game.EXE", "b")],
            vec![(".installed-release", "fake")],
            vec![(".INSTALLED-METADATA.JSON", "fake")],
            vec![("nested/.installed-metadata.pending", "fake")],
        ] {
            let zip = t.path().join("bad.zip");
            package(&zip, &entries);
            assert!(extract(&zip, &t.path().join("out")).is_err());
        }
    }
    #[test]
    fn install_update_and_uninstall_keep_player_files() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("test-game");
        let zip = t.path().join("game.zip");
        package(&zip, &[("test-game.exe", "first"), ("obsolete.dat", "old")]);
        install_zip(&game(), &zip, &dir).unwrap();
        fs::write(dir.join("save.json"), "progress").unwrap();
        package(&zip, &[("test-game.exe", "second")]);
        install_zip(&game(), &zip, &dir).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("save.json")).unwrap(),
            "progress"
        );
        assert!(!dir.join("obsolete.dat").exists());
        assert_eq!(
            fs::read_to_string(dir.join(".installed-release")).unwrap(),
            "new-build"
        );
        uninstall(&dir).unwrap();
        assert!(dir.join("save.json").exists());
        assert!(!dir.join("test-game.exe").exists());
    }
    #[test]
    fn dates_and_version_identity_change_only_with_a_successful_install() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("test-game");
        let archive = temp.path().join("game.zip");
        let mut initial = game();
        initial.version = "1.2.3".into();
        initial.released_at = Some("2026-10-01T12:30:00Z".into());
        package(&archive, &[("test-game.exe", "initial")]);
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        install_zip(&initial, &archive, &target).unwrap();
        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let mut receipt = metadata(&target, "new-build").unwrap();
        assert_eq!(receipt.version, "1.2.3");
        assert_eq!(receipt.released_at, initial.released_at);
        assert!((before..=after).contains(&u128::from(receipt.installed_at)));
        // An old receipt makes a successful replacement observable without
        // relying on clock sleeps or filesystem timestamps.
        receipt.installed_at = 1;
        fs::write(
            target.join(METADATA_RECEIPT),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        let original = fs::read(target.join(METADATA_RECEIPT)).unwrap();
        fs::write(target.join("player-save.txt"), "keep progress").unwrap();
        package(&archive, &[("../bad.exe", "bad")]);
        assert!(install_zip(&initial, &archive, &target).is_err());
        assert_eq!(fs::read(target.join(METADATA_RECEIPT)).unwrap(), original);
        assert_eq!(metadata(&target, "new-build").unwrap().installed_at, 1);
        let mut updated = initial;
        updated.package.release = "second-build".into();
        updated.version = "2.0.0".into();
        updated.released_at = Some("2026-10-17T19:00:00Z".into());
        package(&archive, &[("test-game.exe", "updated")]);
        install_zip(&updated, &archive, &target).unwrap();
        let receipt = metadata(&target, "second-build").unwrap();
        assert_eq!(receipt.version, "2.0.0");
        assert_eq!(receipt.released_at, updated.released_at);
        assert!(receipt.installed_at > 1);
        assert!(metadata(&target, "new-build").is_none());
        assert_eq!(
            fs::read_to_string(target.join("player-save.txt")).unwrap(),
            "keep progress"
        );
        uninstall(&target).unwrap();
        assert!(!target.join(METADATA_RECEIPT).exists());
        assert!(target.join("player-save.txt").exists());
    }

    #[test]
    fn legacy_and_invalid_receipts_keep_dates_unknown_without_writing_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("test-game.exe"), "legacy").unwrap();
        fs::write(temp.path().join(".installed-release"), "legacy-release").unwrap();
        assert!(metadata(temp.path(), "legacy-release").is_none());
        assert!(!temp.path().join(METADATA_RECEIPT).exists());
        for invalid in [
            "not json".to_string(),
            serde_json::json!({"schema_version":1,"release":"another-release","version":"1","released_at":null,"installed_at":1234}).to_string(),
            serde_json::json!({"schema_version":1,"release":"legacy-release","version":"1","released_at":null,"installed_at":0}).to_string(),
        ] {
            fs::write(temp.path().join(METADATA_RECEIPT), &invalid).unwrap();
            assert!(metadata(temp.path(), "legacy-release").is_none());
            assert_eq!(fs::read_to_string(temp.path().join(METADATA_RECEIPT)).unwrap(), invalid);
        }
    }
    #[test]
    fn bundled_feed_is_trusted_and_external_urls_rejected() {
        let mut c: crate::catalog::Catalog =
            serde_json::from_str(include_str!("../../public/catalog.json")).unwrap();
        crate::catalog::validate(&c).unwrap();
        for url in [
            "https://evil.example/x.exe",
            "http://github.com/kevstermcgee/BlueEngineGames/releases/download/tag/a.zip",
            "https://github.com/other/BlueEngineGames/releases/download/tag/a.zip",
            "https://github.com/kevstermcgee/BlueEngineGames/releases/latest/download/a.zip",
        ] {
            c.games[0].package.url = url.into();
            assert!(crate::catalog::validate(&c).is_err(), "{url}");
        }
    }
    #[test]
    #[ignore = "Downloads a published Windows game; run explicitly in Windows release CI"]
    fn published_package_round_trip() {
        let c: crate::catalog::Catalog =
            serde_json::from_str(include_str!("../../public/catalog.json")).unwrap();
        crate::catalog::validate(&c).unwrap();
        let g = c
            .games
            .iter()
            .filter(|g| g.package.kind == "zip")
            .min_by_key(|g| g.package.size)
            .unwrap();
        let t = tempfile::tempdir().unwrap();
        let zip = t.path().join("game.zip");
        download(g, &zip, &AtomicBool::new(false), |_, _| {}).unwrap();
        let dir = t.path().join(&g.slug);
        install_zip(g, &zip, &dir).unwrap();
        assert!(playable(&dir, &g.slug).is_some());
        fs::write(dir.join("player-save.txt"), "keep me").unwrap();
        install_zip(g, &zip, &dir).unwrap();
        uninstall(&dir).unwrap();
        assert!(dir.join("player-save.txt").exists());
    }
}

#[cfg(windows)]
pub fn install_external(game: &Game, setup: &Path, target: &Path) -> Result<(), String> {
    let status = std::process::Command::new(setup)
        .args([
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/SP-",
            "/NOICONS",
            // Inno's /NOICONS controls Start Menu icons, not desktop tasks.
            // Override a remembered desktop choice without disabling other tasks.
            "/MERGETASKS=!desktopicon",
        ])
        .arg(format!("/DIR={}", target.display()))
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("The game's installer returned {status}. Your previous installation may still be available."));
    }
    if playable(target, &game.slug).is_none() {
        return Err("Installer finished but no playable executable was found".into());
    }
    fs::write(target.join(".installed-release"), &game.package.release)
        .map_err(|e| e.to_string())?;
    write_metadata(game, target)
}

#[cfg(windows)]
pub fn uninstall_external(dir: &Path) -> Result<(), String> {
    let status = std::process::Command::new(dir.join("unins000.exe"))
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("The game's uninstaller did not finish".into());
    }
    let _ = fs::remove_file(dir.join(".installed-release"));
    let _ = fs::remove_file(dir.join(METADATA_RECEIPT));
    let _ = fs::remove_file(dir.join(METADATA_PENDING));
    Ok(())
}

#[cfg(all(test, windows))]
mod independent_installer_tests {
    use super::*;
    use std::process::Command;

    fn powershell(script: &str, shortcut: Option<&Path>) -> Result<String, String> {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); {script}"
            ),
        ]);
        if let Some(path) = shortcut {
            command.env("BLUEENGINE_TEST_SHORTCUT", path);
        }
        let output = command.output().map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err("Could not inspect the Windows Desktop shortcut fixture".into());
        }
        String::from_utf8(output.stdout)
            .map(|value| value.trim().to_string())
            .map_err(|error| error.to_string())
    }

    fn shortcut_points_to(shortcut: &Path, executable: &Path) -> bool {
        if !shortcut.is_file() {
            return false;
        }
        let target = powershell(
            "(New-Object -ComObject WScript.Shell).CreateShortcut($env:BLUEENGINE_TEST_SHORTCUT).TargetPath",
            Some(shortcut),
        );
        match (target, fs::canonicalize(executable)) {
            (Ok(path), Ok(expected)) => {
                fs::canonicalize(path).is_ok_and(|actual| actual == expected)
            }
            _ => false,
        }
    }

    struct TestDesktopShortcut {
        path: PathBuf,
        executable: PathBuf,
    }
    impl Drop for TestDesktopShortcut {
        fn drop(&mut self) {
            // Only this test's newly created shortcut may be removed, even on
            // assertion failure. Never clean up a pre-existing player shortcut.
            if shortcut_points_to(&self.path, &self.executable) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    #[test]
    #[ignore = "Downloads both independent games; remembered-shortcut proof requires an isolated GitHub-hosted Windows runner"]
    fn independent_installers_round_trip() {
        let catalog: crate::catalog::Catalog =
            serde_json::from_str(include_str!("../../public/catalog.json")).unwrap();
        let isolated_runner = std::env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true")
            && std::env::var("RUNNER_ENVIRONMENT").is_ok_and(|value| value == "github-hosted");
        let desktop = PathBuf::from(
            powershell(
                "[Environment]::GetFolderPath('DesktopDirectory', 'Create')",
                None,
            )
            .unwrap(),
        );
        assert!(
            desktop.is_dir(),
            "Windows must resolve the current user's Desktop"
        );
        let mut proof = Vec::new();
        for game in catalog
            .games
            .iter()
            .filter(|g| g.package.kind == "installer")
        {
            let temp = tempfile::tempdir().unwrap();
            let setup = temp.path().join("setup.exe");
            download(game, &setup, &AtomicBool::new(false), |_, _| {}).unwrap();
            let target = temp.path().join(&game.slug);
            let shortcut = desktop.join(format!("{}.lnk", game.name));
            let existing_shortcut = match fs::read(&shortcut) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => panic!("Could not safely inspect existing shortcut: {error}"),
            };
            if isolated_runner {
                assert!(
                    existing_shortcut.is_none(),
                    "Shortcut proof refuses to modify an existing desktop shortcut"
                );
            }
            install_external(game, &setup, &target).unwrap();
            assert_eq!(
                fs::read(&shortcut).ok(),
                existing_shortcut,
                "Launcher installation must not create or replace desktop shortcuts"
            );
            let initial_metadata = metadata(&target, &game.package.release).unwrap();
            assert_eq!(initial_metadata.released_at, game.released_at);
            fs::write(target.join("player-save.txt"), "keep me").unwrap();
            let _test_shortcut = if isolated_runner {
                // Seed Inno's real remembered task through an explicit opt-in,
                // using only a disposable hosted runner and temporary game dir.
                let fixture = TestDesktopShortcut {
                    path: shortcut.clone(),
                    executable: playable(&target, &game.slug).unwrap(),
                };
                let status = Command::new(&setup)
                    .args([
                        "/VERYSILENT",
                        "/SUPPRESSMSGBOXES",
                        "/NORESTART",
                        "/SP-",
                        "/NOICONS",
                        "/TASKS=desktopicon",
                    ])
                    .arg(format!("/DIR={}", target.display()))
                    .status()
                    .unwrap();
                assert!(status.success());
                assert!(
                    shortcut_points_to(&fixture.path, &fixture.executable),
                    "Explicit desktop opt-in must create this fixture's shortcut"
                );
                fs::remove_file(&fixture.path).unwrap();
                Some(fixture)
            } else {
                println!("Remembered desktop task proof is skipped outside an isolated GitHub-hosted Windows runner");
                None
            };
            install_external(game, &setup, &target).unwrap();
            assert_eq!(
                fs::read(&shortcut).ok(),
                existing_shortcut,
                "Launcher update must not recreate a remembered desktop shortcut"
            );
            assert!(
                metadata(&target, &game.package.release)
                    .unwrap()
                    .installed_at
                    >= initial_metadata.installed_at
            );
            assert_eq!(
                fs::read_to_string(target.join("player-save.txt")).unwrap(),
                "keep me"
            );
            uninstall_external(&target).unwrap();
            assert!(target.join("player-save.txt").exists());
            assert!(playable(&target, &game.slug).is_none());
            assert!(!target.join(METADATA_RECEIPT).exists());
            proof.push(serde_json::json!({
                "game":game.slug, "fresh_launcher_install_no_desktop_shortcut":true,
                "remembered_desktop_task_reinstall_verified":isolated_runner,
                "preexisting_desktop_shortcut_present":existing_shortcut.is_some(),
                "save_preserved":true,
            }));
        }
        if isolated_runner {
            let proof_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../launcher-smoke");
            fs::create_dir_all(&proof_dir).unwrap();
            fs::write(
                proof_dir.join("desktop-shortcut-policy.json"),
                serde_json::to_vec_pretty(&serde_json::json!({"schema_version":1,"games":proof}))
                    .unwrap(),
            )
            .unwrap();
        }
    }
}

pub fn recover_interrupted(target: &Path, slug: &str) -> Result<bool, String> {
    let backup = target
        .parent()
        .ok_or("Missing installation parent")?
        .join(format!(".{slug}-backup"));
    if !target.exists() && backup.is_dir() {
        fs::rename(backup, target).map_err(|e| e.to_string())?;
        return Ok(true);
    }
    Ok(false)
}
#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn interrupted_commit_restores_previous_game() {
        let temp = tempfile::tempdir().unwrap();
        let backup = temp.path().join(".test-backup");
        fs::create_dir(&backup).unwrap();
        fs::write(backup.join("test.exe"), "previous version").unwrap();
        fs::write(backup.join(".installed-release"), "old-release").unwrap();
        let receipt = InstallationMetadata {
            schema_version: 1,
            release: "old-release".into(),
            version: "0.9".into(),
            released_at: Some("2026-10-01T00:00:00Z".into()),
            installed_at: 1_700_000_000_000,
        };
        fs::write(
            backup.join(METADATA_RECEIPT),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        let target = temp.path().join("test");
        assert!(recover_interrupted(&target, "test").unwrap());
        assert_eq!(
            fs::read_to_string(target.join("test.exe")).unwrap(),
            "previous version"
        );
        assert!(!recover_interrupted(&target, "test").unwrap());
        assert_eq!(metadata(&target, "old-release").unwrap(), receipt);
    }
}
