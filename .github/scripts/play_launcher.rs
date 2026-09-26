#![windows_subsystem = "windows"]

use std::fs;
use std::path::Path;
use std::process::Command;

fn launch() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory = executable.parent().ok_or("launcher has no parent directory")?;
    let engine_name = fs::read_to_string(directory.join("engine.name"))
        .map_err(|error| format!("cannot read engine.name: {error}"))?;
    let arguments = fs::read_to_string(directory.join("launch.args"))
        .map_err(|error| format!("cannot read launch.args: {error}"))?;
    let engine = directory.join(engine_name.trim());
    if !engine.is_file() {
        return Err(format!("engine executable is missing: {}", engine.display()));
    }
    Command::new(engine)
        .current_dir(directory)
        .args(arguments.lines().filter(|line| !line.is_empty()))
        .spawn()
        .map_err(|error| format!("cannot start the game: {error}"))?;
    Ok(())
}

fn main() {
    if let Err(error) = launch() {
        let directory = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let _ = fs::write(directory.join("launcher-error.txt"), format!("{error}\n"));
    }
}
