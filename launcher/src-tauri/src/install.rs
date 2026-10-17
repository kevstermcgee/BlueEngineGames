use crate::catalog::{client, Game};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
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
    if parts
        .iter()
        .any(|s| [".installed-files.tsv", ".installed-release"].contains(s))
    {
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
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
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
            } else if ![".installed-release", ".installed-files.tsv"].contains(&key.as_str())
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
