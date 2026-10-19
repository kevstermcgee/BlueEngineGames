use blueengine_launcher::catalog::{self, Catalog};
use std::{env, fs, path::PathBuf, process::ExitCode};

fn check() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let path = args
        .next()
        .filter(|_| args.next().is_none())
        .map(PathBuf::from)
        .ok_or("Usage: check-catalog <catalog.json>")?;
    let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let catalog: Catalog =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    catalog::validate(&catalog).map_err(|error| format!("{}: {error}", path.display()))?;
    println!(
        "Validated {} launcher games in {}",
        catalog.games.len(),
        path.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    match check() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
