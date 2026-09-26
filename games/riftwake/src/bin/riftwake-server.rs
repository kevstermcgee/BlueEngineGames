use riftwake::{server::Server, DEFAULT_KEY, DEFAULT_SERVER};
use std::sync::{atomic::AtomicBool, Arc};

fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let address = value_after(&args, "--listen").unwrap_or_else(|| DEFAULT_SERVER.into());
    let key = value_after(&args, "--key").unwrap_or_else(|| DEFAULT_KEY.into());
    Server::bind(&address, key)?.run(Arc::new(AtomicBool::new(false)), None)
}
fn value_after(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|x| x == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
