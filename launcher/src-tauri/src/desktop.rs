use crate::{
    catalog::{self, Catalog, Game},
    install,
};
use fs2::FileExt;
use serde::Serialize;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    path::PathBuf,
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{Emitter, Manager, State};
struct Library {
    root: PathBuf,
    catalog: Mutex<Catalog>,
    children: Mutex<HashMap<String, Child>>,
    busy: AtomicBool,
    active: Mutex<Option<String>>,
    cancel: AtomicBool,
}
#[derive(Serialize)]
struct Installed {
    slug: String,
    installed: bool,
    release: String,
    running: bool,
    path: String,
}
#[derive(Serialize)]
struct Snapshot {
    catalog: Catalog,
    installed: Vec<Installed>,
    offline: bool,
    message: String,
    root: String,
}
#[derive(Clone, Serialize)]
struct Progress {
    slug: String,
    phase: String,
    received: u64,
    total: u64,
    message: String,
}
impl Library {
    fn game(&self, slug: &str) -> Result<Game, String> {
        self.catalog
            .lock()
            .map_err(|e| e.to_string())?
            .games
            .iter()
            .find(|g| g.slug == slug)
            .cloned()
            .ok_or("Unknown game".into())
    }
    fn dir(&self, slug: &str) -> PathBuf {
        let standard = self.root.join(slug);
        let old = self
            .root
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(&self.root)
            .join("BlueEngineLauncher/games")
            .join(slug);
        if !standard.exists() && old.exists() {
            old
        } else {
            standard
        }
    }
    fn running(&self, slug: &str) -> bool {
        let mut children = self.children.lock().unwrap();
        if let Some(child) = children.get_mut(slug) {
            if child.try_wait().ok().flatten().is_none() {
                return true;
            }
            children.remove(slug);
        }
        let dir = self.dir(slug);
        let mut processes = sysinfo::System::new();
        processes.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        processes
            .processes()
            .values()
            .any(|p| p.exe().is_some_and(|exe| exe.starts_with(&dir)))
    }
    fn snapshot(&self, offline: bool, message: String) -> Snapshot {
        let catalog = self.catalog.lock().unwrap().clone();
        let mut processes = sysinfo::System::new();
        processes.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let mut children = self.children.lock().unwrap();
        children.retain(|_, child| child.try_wait().ok().flatten().is_none());
        let installed = catalog
            .games
            .iter()
            .map(|g| {
                let dir = self.dir(&g.slug);
                Installed {
                    slug: g.slug.clone(),
                    installed: install::playable(&dir, &g.slug).is_some(),
                    release: fs::read_to_string(dir.join(".installed-release"))
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    running: children.contains_key(&g.slug)
                        || processes
                            .processes()
                            .values()
                            .any(|p| p.exe().is_some_and(|exe| exe.starts_with(&dir))),
                    path: dir.display().to_string(),
                }
            })
            .collect();
        Snapshot {
            catalog,
            installed,
            offline,
            message,
            root: self.root.display().to_string(),
        }
    }
}
#[tauri::command]
async fn refresh(state: State<'_, Library>) -> Result<Snapshot, String> {
    let root = state.root.clone();
    let result = tauri::async_runtime::spawn_blocking(catalog::fetch)
        .await
        .map_err(|e| e.to_string())?;
    match result {
        Ok(c) => {
            let bytes = serde_json::to_vec(&c).map_err(|e| e.to_string())?;
            let tmp = root.join("catalog.tmp");
            fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
            let dest = root.join("catalog.json");
            let previous = root.join("catalog.previous.json");
            if previous.exists() {
                fs::remove_file(&previous).map_err(|e| e.to_string())?;
            }
            if dest.exists() {
                fs::rename(&dest, &previous).map_err(|e| e.to_string())?;
            }
            if let Err(error) = fs::rename(tmp, &dest) {
                let _ = fs::rename(previous, &dest);
                return Err(error.to_string());
            }
            let _ = fs::remove_file(previous);
            *state.catalog.lock().unwrap() = c;
            Ok(state.snapshot(false, String::new()))
        }
        Err(e) => Ok(state.snapshot(
            true,
            format!(
                "Using your saved library. Connect to the internet for new games and updates. {e}"
            ),
        )),
    }
}
#[tauri::command]
fn snapshot(state: State<'_, Library>) -> Snapshot {
    state.snapshot(false, String::new())
}
#[tauri::command]
fn launch(slug: String, state: State<'_, Library>) -> Result<(), String> {
    state.game(&slug)?;
    if state.active.lock().unwrap().as_deref() == Some(slug.as_str()) {
        return Err("Wait for this game to finish installing".into());
    }
    if state.running(&slug) {
        return Err("This game is already running".into());
    }
    let dir = state.dir(&slug);
    let exe = install::playable(&dir, &slug).ok_or("Install this game first")?;
    let child = Command::new(exe)
        .current_dir(dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    state.children.lock().unwrap().insert(slug, child);
    Ok(())
}
fn lock(dir: &std::path::Path, slug: &str) -> Result<std::fs::File, String> {
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(format!(".{slug}.lock")))
        .map_err(|e| e.to_string())?;
    f.try_lock_exclusive()
        .map_err(|_| "Another updater is managing this game".to_string())?;
    Ok(f)
}
#[tauri::command]
fn install_game(
    slug: String,
    app: tauri::AppHandle,
    state: State<'_, Library>,
) -> Result<(), String> {
    let game = state.game(&slug)?;
    if state.running(&slug) {
        return Err("Close the game before updating".into());
    }
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("An installation is already in progress".into());
    }
    *state.active.lock().unwrap() = Some(slug.clone());
    state.cancel.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let state = app.state::<Library>();
        let dir = state.dir(&slug);
        let emit = |phase: &str, received, total, message: String| {
            let _ = app.emit(
                "download",
                Progress {
                    slug: slug.clone(),
                    phase: phase.into(),
                    received,
                    total,
                    message,
                },
            );
        };
        let result = (|| {
            let parent = dir.parent().ok_or("Invalid game folder")?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let _guard = lock(parent, &slug)?;
            let download = parent.join(format!(
                ".{slug}-download.{}",
                if game.package.kind == "zip" {
                    "zip"
                } else {
                    "exe"
                }
            ));
            let result = (|| {
                emit("downloading", 0, game.package.size, String::new());
                install::download(&game, &download, &state.cancel, |n, total| {
                    emit("downloading", n, total, String::new())
                })?;
                if state.cancel.load(Ordering::SeqCst) {
                    return Err("Download cancelled".into());
                }
                emit(
                    "installing",
                    game.package.size,
                    game.package.size,
                    String::new(),
                );
                if game.package.kind == "zip" {
                    install::install_zip(&game, &download, &dir)?;
                } else {
                    install::install_external(&game, &download, &dir)?;
                }
                Ok::<_, String>(())
            })();
            let _ = fs::remove_file(download);
            result
        })();
        *state.active.lock().unwrap() = None;
        state.busy.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => emit(
                "complete",
                game.package.size,
                game.package.size,
                "Ready to play".into(),
            ),
            Err(e) => emit("error", 0, game.package.size, e),
        }
    });
    Ok(())
}
#[tauri::command]
fn cancel_download(state: State<'_, Library>) {
    state.cancel.store(true, Ordering::SeqCst);
}
#[tauri::command]
fn uninstall_game(slug: String, state: State<'_, Library>) -> Result<(), String> {
    let game = state.game(&slug)?;
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("Wait for the current installation".into());
    }
    let result = (|| {
        if state.running(&slug) {
            return Err("Close the game before uninstalling".into());
        }
        let dir = state.dir(&slug);
        let _guard = lock(dir.parent().ok_or("Invalid folder")?, &slug)?;
        let uninstaller = dir.join("unins000.exe");
        if game.package.kind == "installer" && uninstaller.is_file() {
            install::uninstall_external(&dir)
        } else {
            install::uninstall(&dir)
        }
    })();
    state.busy.store(false, Ordering::SeqCst);
    result
}
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let root = PathBuf::from(
                std::env::var_os("LOCALAPPDATA")
                    .ok_or("Windows local app data folder is unavailable")?,
            )
            .join("BlueEngine/Games");
            fs::create_dir_all(&root)?;
            let cached = fs::read(root.join("catalog.json"))
                .or_else(|_| fs::read(root.join("catalog.previous.json")))
                .ok()
                .and_then(|b| serde_json::from_slice::<Catalog>(&b).ok())
                .filter(|c| catalog::validate(c).is_ok());
            let bundled: Catalog = serde_json::from_str(include_str!("../../public/catalog.json"))?;
            catalog::validate(&bundled).map_err(std::io::Error::other)?;
            app.manage(Library {
                root,
                catalog: Mutex::new(cached.unwrap_or(bundled)),
                children: Mutex::new(HashMap::new()),
                busy: AtomicBool::new(false),
                active: Mutex::new(None),
                cancel: AtomicBool::new(false),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            refresh,
            snapshot,
            launch,
            install_game,
            cancel_download,
            uninstall_game
        ])
        .run(tauri::generate_context!())
        .expect("Could not start BlueEngine Games");
}
