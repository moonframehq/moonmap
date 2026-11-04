mod demo;
mod location;
mod net;
mod provider;
mod shield;
mod street_view;

use {
    anyhow::{Context, Result},
    fs2::FileExt,
    moonframe::Moonframe,
    std::{fs::OpenOptions, path::PathBuf, process::ExitCode},
};

#[derive(moonframe::rust_embed::RustEmbed)]
#[crate_path = "moonframe::rust_embed"]
#[folder = "app"]
struct AppFiles;

/// Where the library lives: `MOONMAP_DATA_DIR`, else Application Support.
fn root(demo: bool) -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("MOONMAP_DATA_DIR") {
        return Ok(root.into());
    }
    let home =
        PathBuf::from(std::env::var_os("HOME").context("HOME is unset; set MOONMAP_DATA_DIR")?);
    Ok(home
        .join("Library/Application Support")
        .join(if demo { "moonmap-demo" } else { "moonmap" }))
}

/// The folder `ui.image` may show files from, as Moonframe works it out.
fn images(demo: bool) -> PathBuf {
    let name = if demo { "moonmap-demo" } else { "moonmap" };
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    cache.join(name).join("images")
}

fn run() -> Result<ExitCode> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let check = args.iter().any(|arg| arg == "--check");
    let demo = check || std::env::var_os("MOONMAP_DEMO").is_some();
    let temporary = if check {
        Some(tempfile::tempdir()?)
    } else {
        None
    };
    let root = match &temporary {
        Some(dir) => dir.path().to_owned(),
        None => root(demo)?,
    };
    std::fs::create_dir_all(&root)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("app.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("Moonmap is already open on {}", root.display()))?;
    let library = root.join("library.json");
    if demo && !library.exists() {
        demo::library().save(&library)?;
    }
    let mut app = Moonframe::new(if demo { "moonmap-demo" } else { "moonmap" })
        .title("Moonmap")
        .app_id("dev.moonmap.app")
        .version(env!("CARGO_PKG_VERSION"))
        .keep(["feedback"])
        .embedded_app::<AppFiles>();
    if cfg!(debug_assertions) {
        app = app.app_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/app"));
    }
    let images = images(demo);
    // Offline checks never reach the web services.
    let online = !check;
    Ok(app
        .native("maps", move || {
            provider::Maps::new(library.clone(), online).with_images(images.clone())
        })
        .view("street-view", street_view::StreetView::new)
        .view("shield", shield::Shield::new)
        .run())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Moonmap: {error:#}");
            ExitCode::FAILURE
        },
    }
}
