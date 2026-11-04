use {
    super::*,
    std::sync::{Arc, Mutex},
};

fn message(fields: &[(&str, Data)]) -> Data {
    Data::map(fields.iter().map(|(key, value)| (*key, value.clone())))
}

fn text(value: &str) -> Data {
    Data::String(value.into())
}

/// An offline provider over the demo library, with what it published.
fn provider() -> Result<(tempfile::TempDir, Maps, Handle, Arc<Mutex<u32>>)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("library.json");
    crate::demo::library().save(&path)?;
    let published = Arc::new(Mutex::new(0));
    let count = Arc::clone(&published);
    let host = Handle::new(
        "maps",
        dir.path(),
        Data::Nil,
        Arc::new(move |_| {
            if let Ok(mut count) = count.lock() {
                *count += 1;
            }
        }),
    );
    let mut maps = Maps::new(path, false);
    maps.start(&host).map_err(anyhow::Error::msg)?;
    Ok((dir, maps, host, published))
}

#[test]
fn opens_pins_saves_and_persists_known_places() -> Result<()> {
    let (_dir, mut maps, host, published) = provider()?;
    let open = message(&[("action", text("open")), ("id", text("demo:tartine"))]);
    maps.receive(&host, "shell", open);
    assert_eq!(maps.error, "");
    let snapshot = maps.snapshot();
    assert_eq!(snapshot["selected"]["name"], "Tartine Bakery");
    assert_eq!(snapshot["recents"][0]["place"]["id"], "demo:tartine");
    assert_eq!(snapshot["selected"]["pinned"], false);
    maps.receive(
        &host,
        "shell",
        message(&[("action", text("pin")), ("id", text("demo:tartine"))]),
    );
    maps.receive(
        &host,
        "shell",
        message(&[("action", text("save")), ("id", text("demo:tartine"))]),
    );
    let snapshot = maps.snapshot();
    assert_eq!(snapshot["selected"]["pinned"], true);
    assert_eq!(snapshot["selected"]["saved"], true);
    assert_eq!(snapshot["added"][0]["id"], "demo:tartine");
    maps.save();
    let reloaded = Library::load(&maps.path)?;
    assert!(reloaded.is_pinned("demo:tartine") && reloaded.is_saved("demo:tartine"));
    assert!(*published.lock().map_err(|_| anyhow::anyhow!("poisoned"))? >= 4);
    Ok(())
}

#[test]
fn rejects_places_it_never_published() -> Result<()> {
    let (_dir, mut maps, host, _) = provider()?;
    let forged = message(&[("action", text("pin")), ("id", text("osm:node/1"))]);
    maps.receive(&host, "shell", forged);
    assert!(maps.error.contains("Unknown place"), "{}", maps.error);
    assert!(!maps.library.is_pinned("osm:node/1"));
    maps.receive(&host, "shell", message(&[("action", text("teleport"))]));
    assert!(maps.error.contains("Unknown action"));
    Ok(())
}

#[test]
fn guides_routes_styles_and_keys() -> Result<()> {
    let (_dir, mut maps, host, _) = provider()?;
    let new = message(&[
        ("action", text("guide_new")),
        ("name", text("Weekend")),
        ("id", text("demo:tartine")),
    ]);
    maps.receive(&host, "shell", new);
    let guide = maps
        .library
        .guides
        .last()
        .map(|guide| guide.id)
        .context("No guide")?;
    assert_eq!(
        maps.library.guides.last().map(|guide| guide.places.len()),
        Some(1)
    );
    maps.receive(
        &host,
        "shell",
        message(&[
            ("action", text("guide_toggle")),
            ("guide", Data::Number(guide as f64)),
            ("id", text("demo:tartine")),
        ]),
    );
    assert_eq!(
        maps.library.guides.last().map(|guide| guide.places.len()),
        Some(0)
    );
    let routes = maps.library.routes.len();
    maps.receive(
        &host,
        "shell",
        message(&[
            ("action", text("route_save")),
            ("id", text("demo:tartine")),
            ("mode", text("teleport")),
        ]),
    );
    assert_eq!(maps.library.routes.len(), routes + 1);
    assert_eq!(maps.library.routes[0].mode, "car");
    maps.receive(
        &host,
        "shell",
        message(&[("action", text("style")), ("style", text("nope"))]),
    );
    assert_eq!(maps.snapshot()["style"], "esri-streets");
    maps.receive(
        &host,
        "shell",
        message(&[("action", text("style")), ("style", text("stadia-smooth"))]),
    );
    maps.receive(
        &host,
        "shell",
        message(&[
            ("action", text("key")),
            ("style", text("stadia-smooth")),
            ("key", text(" abc ")),
        ]),
    );
    let snapshot = maps.snapshot();
    assert_eq!(
        (snapshot["style"].as_str(), snapshot["key"].as_str()),
        (Some("stadia-smooth"), Some("abc"))
    );
    maps.receive(
        &host,
        "shell",
        message(&[
            ("action", text("moved")),
            ("lat", Data::Number(40.0)),
            ("lon", Data::Number(-74.0)),
            ("zoom", Data::Number(12.0)),
        ]),
    );
    assert_eq!(maps.snapshot()["location_status"], "offline");
    assert_eq!(maps.snapshot()["located_before"], false);
    maps.answers.send(Answer::Location(Located::Fix {
        lat: 40.0,
        lon: -74.0,
        meters: 4.6,
    }))?;
    maps.answers.send(Answer::Location(Located::Approximate))?;
    maps.tick(&host);
    let snapshot = maps.snapshot();
    assert_eq!(
        (
            snapshot["location"]["lat"].as_f64(),
            snapshot["accuracy"].as_f64()
        ),
        (Some(40.0), Some(5.0))
    );
    // The next launch opens here, without waiting for the service.
    assert_eq!(snapshot["located_before"], true);
    assert_eq!(snapshot["ready"], true);
    // Fixes keep coming on an approximate grant; they still count.
    assert_eq!(snapshot["location_status"], "located");
    maps.answers.send(Answer::Location(Located::Denied))?;
    maps.tick(&host);
    assert_eq!(maps.snapshot()["location_status"], "denied");
    Ok(())
}
