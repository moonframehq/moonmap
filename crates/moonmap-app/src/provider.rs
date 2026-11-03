//! The `maps` native provider: the user's library, search, suggestions and
//! weather, published as `app.maps`. Messages name places by `id`; the
//! provider only acts on places it has published, so a view cannot make
//! up a place to save.

use {
    crate::{
        location::{self, Located},
        net::{self, Answer},
    },
    anyhow::{Context, Result},
    moonframe::{Data, Handle, Native},
    moonmap_core::{
        Coordinate, Drive, Library, Place, Recent, STYLES, Weather, distance_label,
        distance_meters, duration_label,
    },
    serde_json::{Value, json},
    std::{
        collections::HashMap,
        path::PathBuf,
        sync::mpsc::{self, Receiver, Sender},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    },
};

/// How many pinned places get a driving suggestion.
const SUGGESTIONS: usize = 2;
/// Weather is asked again after this long, or once the map moves this far.
const WEATHER_AGE: Duration = Duration::from_secs(15 * 60);
const WEATHER_MOVE_METERS: f64 = 5_000.0;
/// Drive times are asked again once My Location moves this far.
const DRIVES_MOVE_METERS: f64 = 200.0;
/// Changes are written this long after the last one.
const SAVE_AFTER: Duration = Duration::from_millis(800);

pub struct Maps {
    path: PathBuf,
    online: bool,
    library: Library,
    answers: Sender<Answer>,
    inbox: Receiver<Answer>,
    query: String,
    serial: u64,
    searching: bool,
    results: Vec<Place>,
    selected: Option<Place>,
    drives: HashMap<String, Result<Drive, String>>,
    drives_from: Option<Coordinate>,
    weather: Option<Weather>,
    weather_at: Option<(Coordinate, Instant)>,
    /// The location service's last word; `None` until it says anything.
    located: Option<Located>,
    dirty: Option<Instant>,
    error: String,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// A place as views draw it: its fields plus the badge's icon and color.
fn shown(place: &Place) -> Value {
    json!({
        "id": place.id, "name": place.name, "subtitle": place.subtitle,
        "lat": place.lat, "lon": place.lon,
        "icon": place.category.icon(), "color": place.category.color(),
    })
}

impl Maps {
    pub fn new(path: PathBuf, online: bool) -> Self {
        let (answers, inbox) = mpsc::channel();
        Self {
            path,
            online,
            library: Library::default(),
            answers,
            inbox,
            query: String::new(),
            serial: 0,
            searching: false,
            results: Vec::new(),
            selected: None,
            drives: HashMap::new(),
            drives_from: None,
            weather: None,
            weather_at: None,
            located: None,
            dirty: None,
            error: String::new(),
        }
    }

    fn changed(&mut self) {
        self.dirty = Some(Instant::now());
    }

    /// Every place the view was shown, by id.
    fn known(&self, id: &str) -> Option<Place> {
        let library = &self.library;
        let stamped = library
            .pinned
            .iter()
            .chain(&library.saved)
            .chain(library.guides.iter().flat_map(|guide| &guide.places))
            .map(|entry| &entry.place);
        let routes = library
            .routes
            .iter()
            .flat_map(|route| [&route.from, &route.to]);
        let recents = library.recents.iter().filter_map(|recent| match recent {
            Recent::Place { place, .. } => Some(place),
            Recent::Search { .. } => None,
        });
        self.results
            .iter()
            .chain(self.selected.as_ref())
            .chain(stamped)
            .chain(routes)
            .chain(recents)
            .chain(library.visits.iter().map(|visit| &visit.place))
            .find(|place| place.id == id)
            .cloned()
    }

    fn place(&self, message: &Data) -> Result<Place> {
        let id = message
            .get("id")
            .and_then(Data::as_str)
            .context("No place given")?;
        self.known(id)
            .with_context(|| format!("Unknown place {id}"))
    }

    fn handle(&mut self, message: &Data) -> Result<()> {
        let text = |key| {
            message
                .get(key)
                .and_then(Data::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let number = |key| message.get(key).and_then(Data::as_f64);
        match text("action").as_str() {
            "search" => {
                let query = text("query").trim().to_owned();
                if query.is_empty() {
                    return Ok(());
                }
                self.library.remember(Recent::Search {
                    query: query.clone(),
                    near: String::new(),
                    at: now(),
                });
                self.changed();
                self.start_search(query);
            },
            "clear_search" => {
                self.query.clear();
                self.results.clear();
                self.searching = false;
                self.serial += 1;
            },
            "open" => {
                let place = self.place(message)?;
                self.library.remember(Recent::Place {
                    place: place.clone(),
                    at: now(),
                });
                self.library.visit(place.clone(), now());
                self.selected = Some(place);
                self.changed();
            },
            "drop" => {
                let at = Coordinate::new(
                    number("lat").context("No latitude")?,
                    number("lon").context("No longitude")?,
                );
                self.selected = Some(Place::dropped(at));
            },
            "close" => self.selected = None,
            "pin" => {
                let place = self.place(message)?;
                self.library.toggle_pin(place, now());
                self.drives_from = None;
                self.changed();
            },
            "save" => {
                let place = self.place(message)?;
                self.library.toggle_save(place, now());
                self.changed();
            },
            "guide_new" => {
                let id = self.library.add_guide(&text("name"));
                if !text("id").is_empty() {
                    let place = self.place(message)?;
                    self.library.toggle_in_guide(id, place, now());
                }
                self.changed();
            },
            "guide_toggle" => {
                let guide = number("guide").context("No guide")? as u64;
                let place = self.place(message)?;
                self.library
                    .toggle_in_guide(guide, place, now())
                    .context("That guide is gone")?;
                self.changed();
            },
            "guide_remove" => {
                self.library
                    .remove_guide(number("guide").context("No guide")? as u64);
                self.changed();
            },
            "route_save" => {
                let to = self.place(message)?;
                let from = Place {
                    id: "location".into(),
                    name: "My Location".into(),
                    subtitle: String::new(),
                    lat: self.library.preferences.location.lat,
                    lon: self.library.preferences.location.lon,
                    category: moonmap_core::Category::Home,
                };
                let mode = match text("mode").as_str() {
                    mode @ ("walk" | "bike") => mode.to_owned(),
                    _ => "car".to_owned(),
                };
                self.library.save_route(from, to, &mode, now());
                self.changed();
            },
            "route_remove" => {
                self.library
                    .remove_route(number("route").context("No route")? as u64);
                self.changed();
            },
            "clear_recents" => {
                self.library.clear_recents();
                self.changed();
            },
            "style" => {
                let id = text("style");
                if STYLES.iter().any(|style| style.id == id) {
                    self.library.preferences.style = id;
                    self.changed();
                }
            },
            "key" => {
                let (style, key) = (text("style"), text("key").trim().to_owned());
                if key.is_empty() {
                    self.library.preferences.keys.remove(&style);
                } else {
                    self.library.preferences.keys.insert(style, key);
                }
                self.changed();
            },
            "moved" => {
                let preferences = &mut self.library.preferences;
                if let (Some(lat), Some(lon)) = (number("lat"), number("lon")) {
                    preferences.center = Coordinate::new(lat, lon);
                }
                if let Some(zoom) = number("zoom") {
                    preferences.zoom = zoom;
                }
                self.changed();
            },
            "sidebar" => {
                self.library.preferences.sidebar = message
                    .get("open")
                    .and_then(Data::as_bool)
                    .unwrap_or(!self.library.preferences.sidebar);
                self.changed();
            },
            other => anyhow::bail!("Unknown action {other:?}"),
        }
        Ok(())
    }

    fn start_search(&mut self, query: String) {
        self.serial += 1;
        self.query = query.clone();
        self.results.clear();
        if self.online {
            self.searching = true;
            net::search(
                &self.answers,
                self.serial,
                query,
                self.library.preferences.center,
            );
        }
    }

    /// Asks for what is missing: drives to pinned places, the weather.
    fn refresh(&mut self) {
        if !self.online {
            return;
        }
        let location = self.library.preferences.location;
        let far = self
            .drives_from
            .is_none_or(|from| distance_meters(from, location) > DRIVES_MOVE_METERS);
        if far {
            self.drives_from = Some(location);
            self.drives.clear();
        }
        let wanted: Vec<Place> = self
            .library
            .pinned
            .iter()
            .take(SUGGESTIONS)
            .map(|entry| entry.place.clone())
            .filter(|place| !self.drives.contains_key(&place.id))
            .collect();
        for place in wanted {
            // Pending until it answers, so it is asked once.
            self.drives.insert(place.id.clone(), Err(String::new()));
            net::drive(&self.answers, location, &place);
        }
        let center = self.library.preferences.center;
        let stale = self.weather_at.is_none_or(|(at, when)| {
            when.elapsed() > WEATHER_AGE || distance_meters(at, center) > WEATHER_MOVE_METERS
        });
        if stale {
            self.weather_at = Some((center, Instant::now()));
            net::weather(&self.answers, center);
        }
    }

    fn receive_answers(&mut self) -> bool {
        let mut changed = false;
        while let Ok(answer) = self.inbox.try_recv() {
            changed = true;
            match answer {
                Answer::Search { serial, places } if serial == self.serial => {
                    self.searching = false;
                    match places {
                        Ok(places) => self.results = places,
                        Err(error) => self.error = format!("Search failed: {error}"),
                    }
                },
                Answer::Search { .. } => {},
                Answer::Drive { place, drive } => {
                    self.drives.insert(place, drive);
                },
                Answer::Location(located) => {
                    if let Located::Fix { lat, lon, .. } = located {
                        // Kept, so the next launch starts where the user was.
                        self.library.preferences.location = Coordinate::new(lat, lon);
                        self.changed();
                    }
                    // An approximate grant still delivers fixes; keep them.
                    if !(located == Located::Approximate
                        && matches!(self.located, Some(Located::Fix { .. })))
                    {
                        self.located = Some(located);
                    }
                },
                Answer::Weather { at, weather } => match weather {
                    Ok(weather) => self.weather = Some(weather),
                    // Asked again once the map moves or time passes.
                    Err(_) => self.weather_at = Some((at, Instant::now())),
                },
            }
        }
        changed
    }

    fn suggestions(&self) -> Vec<Value> {
        self.library
            .pinned
            .iter()
            .take(SUGGESTIONS)
            .filter_map(|entry| {
                let place = &entry.place;
                let drive = self.drives.get(&place.id)?.as_ref().ok()?;
                let detail = if drive.roads.is_empty() {
                    format!("Drive, {}", distance_label(drive.meters))
                } else {
                    format!("Take {}, {}", drive.roads, distance_label(drive.meters))
                };
                Some(json!({
                    "title": format!("{} to {}", duration_label(drive.seconds), place.name),
                    "subtitle": detail,
                    "place": shown(place),
                }))
            })
            .collect()
    }

    fn snapshot(&self) -> Value {
        let library = &self.library;
        let preferences = &library.preferences;
        let style = moonmap_core::style(&preferences.style);
        let stamped = |list: &[moonmap_core::Stamped]| {
            list.iter()
                .map(|entry| shown(&entry.place))
                .collect::<Vec<_>>()
        };
        let recents: Vec<Value> = library
            .recents
            .iter()
            .map(|recent| match recent {
                Recent::Place { place, .. } => json!({ "kind": "place", "place": shown(place) }),
                Recent::Search { query, near, .. } => json!({
                    "kind": "search", "query": query,
                    "subtitle": if near.is_empty() { "Search" } else { near.as_str() },
                }),
            })
            .collect();
        let selected = self.selected.as_ref().map(|place| {
            let mut value = shown(place);
            let meters = distance_meters(preferences.location, place.coordinate());
            value["pinned"] = library.is_pinned(&place.id).into();
            value["saved"] = library.is_saved(&place.id).into();
            value["distance"] = distance_label(meters).into();
            value["guides"] = library
                .guides
                .iter()
                .filter(|guide| guide.places.iter().any(|entry| entry.place.id == place.id))
                .map(|guide| guide.id)
                .collect::<Vec<_>>()
                .into();
            value
        });
        json!({
            "style": style.id,
            "key": preferences.keys.get(style.id).cloned().unwrap_or_default(),
            "attribution": style.attribution,
            "terms": style.url,
            "styles": STYLES.iter().map(|style| json!({
                "id": style.id, "title": style.title, "detail": style.detail,
                "needs_key": style.key, "has_key": preferences.keys.contains_key(style.id),
                "dark": style.dark, "url": style.url, "swatch": style.swatch,
            })).collect::<Vec<_>>(),
            "location": { "lat": preferences.location.lat, "lon": preferences.location.lon },
            "location_status": match self.located {
                None => if self.online { "waiting" } else { "offline" },
                Some(Located::Fix { .. }) => "located",
                Some(Located::Denied) => "denied",
                Some(Located::Approximate) => "approximate",
                Some(Located::Unbundled) => "unbundled",
            },
            "accuracy": match self.located {
                Some(Located::Fix { meters, .. }) => Some(meters.round()),
                _ => None,
            },
            "center": { "lat": preferences.center.lat, "lon": preferences.center.lon },
            "zoom": preferences.zoom,
            "sidebar": preferences.sidebar,
            "pinned": stamped(&library.pinned),
            "saved": stamped(&library.saved),
            "guides": library.guides.iter().map(|guide| json!({
                "id": guide.id, "name": guide.name, "places": stamped(&guide.places),
            })).collect::<Vec<_>>(),
            "routes": library.routes.iter().map(|route| json!({
                "id": route.id, "mode": route.mode,
                "from": shown(&route.from), "to": shown(&route.to),
            })).collect::<Vec<_>>(),
            "visited": library.visits.iter().map(|visit| {
                let mut value = shown(&visit.place);
                value["count"] = visit.count.into();
                value
            }).collect::<Vec<_>>(),
            "added": library.recently_added(30).iter().map(|entry| shown(&entry.place)).collect::<Vec<_>>(),
            "recents": recents,
            "suggestions": self.suggestions(),
            "query": self.query,
            "searching": self.searching,
            "results": self.results.iter().map(shown).collect::<Vec<_>>(),
            "selected": selected,
            "weather": self.weather,
            "online": self.online,
            "error": self.error,
        })
    }

    fn publish(&self, host: &Handle) {
        host.publish_json(&self.snapshot());
    }

    fn save(&mut self) {
        if let Err(error) = self.library.save(&self.path) {
            self.error = format!("{error}");
        }
        self.dirty = None;
    }
}

impl Native for Maps {
    fn start(&mut self, host: &Handle) -> Result<(), String> {
        self.library = Library::load(&self.path).map_err(|error| error.to_string())?;
        if self.online {
            location::watch(self.answers.clone());
        }
        self.refresh();
        self.publish(host);
        Ok(())
    }

    fn receive(&mut self, host: &Handle, _from: &str, message: Data) {
        self.error.clear();
        if let Err(error) = self.handle(&message) {
            self.error = format!("{error:#}");
        }
        self.refresh();
        self.publish(host);
    }

    fn tick(&mut self, host: &Handle) {
        let mut changed = self.receive_answers();
        if self.dirty.is_some_and(|when| when.elapsed() > SAVE_AFTER) {
            self.save();
            changed = true;
        }
        if changed {
            self.refresh();
            self.publish(host);
        }
    }
}

impl Drop for Maps {
    fn drop(&mut self) {
        if self.dirty.is_some() {
            self.save();
        }
    }
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
