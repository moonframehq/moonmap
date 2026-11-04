use {
    crate::{Coordinate, Place},
    serde::{Deserialize, Serialize},
    std::{
        collections::BTreeMap,
        io::Write as _,
        path::{Path, PathBuf},
    },
};

/// The most recents kept; older ones fall off the end.
pub const MAX_RECENTS: usize = 30;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a Moonmap library: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot encode the library: {0}")]
    Encode(serde_json::Error),
}

/// A place and when it joined a list, in Unix seconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamped {
    pub place: Place,
    pub added: u64,
}

/// A user's named collection of places, as Maps' Guides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub id: u64,
    pub name: String,
    pub places: Vec<Stamped>,
}

/// Directions the user kept: from, to, and how they travel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedRoute {
    pub id: u64,
    pub from: Place,
    pub to: Place,
    /// `car`, `walk` or `bike`, as `ui.map` routes take them.
    pub mode: String,
    pub added: u64,
}

/// Something looked at: a place, or a search typed and submitted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recent {
    Place {
        place: Place,
        at: u64,
    },
    Search {
        query: String,
        near: String,
        at: u64,
    },
}

impl Recent {
    fn key(&self) -> String {
        match self {
            Self::Place { place, .. } => format!("place:{}", place.id),
            Self::Search { query, .. } => format!("search:{}", query.to_lowercase()),
        }
    }
}

/// A place opened, and how often.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Visit {
    pub place: Place,
    pub count: u32,
    pub last: u64,
}

/// What the app remembers between launches besides places.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// The map style's Moonframe provider token.
    pub style: String,
    /// API keys by provider token.
    pub keys: BTreeMap<String, String>,
    /// Where the user last was, from the location service.
    pub location: Coordinate,
    /// Whether `location` is a real fix, not the first run's default.
    pub located: bool,
    /// Where the map last looked.
    pub center: Coordinate,
    pub zoom: f64,
    pub sidebar: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        // Union Square, until the location service answers.
        Self {
            style: crate::STYLES[0].id.into(),
            keys: BTreeMap::new(),
            location: Coordinate::new(37.7880, -122.4075),
            located: false,
            center: Coordinate::new(37.7880, -122.4075),
            zoom: 15.8,
            sidebar: true,
        }
    }
}

/// Everything the user keeps: lists of places, recents and preferences.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub pinned: Vec<Stamped>,
    pub saved: Vec<Stamped>,
    pub guides: Vec<Guide>,
    pub routes: Vec<SavedRoute>,
    pub recents: Vec<Recent>,
    pub visits: Vec<Visit>,
    pub preferences: Preferences,
    next_id: u64,
}

impl Library {
    /// The library at `path`; an absent file is an empty library.
    pub fn load(path: &Path) -> Result<Self, LibraryError> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|source| LibraryError::Parse {
                path: path.to_owned(),
                source,
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(LibraryError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    /// Writes to a sibling file, syncs it, then renames it over `path`, so
    /// a crash leaves the old library or the new one, never half of one.
    pub fn save(&self, path: &Path) -> Result<(), LibraryError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(LibraryError::Encode)?;
        let write = |source| LibraryError::Write {
            path: path.to_owned(),
            source,
        };
        let temporary = path.with_extension("json.partial");
        let mut file = std::fs::File::create(&temporary).map_err(write)?;
        file.write_all(&bytes).map_err(write)?;
        file.sync_all().map_err(write)?;
        std::fs::rename(&temporary, path).map_err(write)
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn is_pinned(&self, id: &str) -> bool {
        self.pinned.iter().any(|entry| entry.place.id == id)
    }

    pub fn is_saved(&self, id: &str) -> bool {
        self.saved.iter().any(|entry| entry.place.id == id)
    }

    /// Pins `place`, or unpins it if it was; returns whether it is now pinned.
    pub fn toggle_pin(&mut self, place: Place, now: u64) -> bool {
        toggle(&mut self.pinned, place, now)
    }

    /// Saves `place`, or forgets it if it was; returns whether it is now saved.
    pub fn toggle_save(&mut self, place: Place, now: u64) -> bool {
        toggle(&mut self.saved, place, now)
    }

    /// Puts `recent` first, dropping an older copy and the oldest past
    /// [`MAX_RECENTS`].
    pub fn remember(&mut self, recent: Recent) {
        let key = recent.key();
        self.recents.retain(|old| old.key() != key);
        self.recents.insert(0, recent);
        self.recents.truncate(MAX_RECENTS);
    }

    pub fn clear_recents(&mut self) {
        self.recents.clear();
    }

    /// Counts a look at `place`, most recent first.
    pub fn visit(&mut self, place: Place, now: u64) {
        let count = match self
            .visits
            .iter()
            .position(|visit| visit.place.id == place.id)
        {
            Some(index) => self.visits.remove(index).count + 1,
            None => 1,
        };
        self.visits.insert(
            0,
            Visit {
                place,
                count,
                last: now,
            },
        );
        self.visits.truncate(200);
    }

    /// Pinned, saved and guide places, newest first, each place once.
    pub fn recently_added(&self, limit: usize) -> Vec<&Stamped> {
        let mut all: Vec<&Stamped> = self
            .pinned
            .iter()
            .chain(&self.saved)
            .chain(self.guides.iter().flat_map(|guide| &guide.places))
            .collect();
        all.sort_by_key(|entry| std::cmp::Reverse(entry.added));
        let mut seen = std::collections::HashSet::new();
        all.retain(|entry| seen.insert(entry.place.id.as_str()));
        all.truncate(limit);
        all
    }

    /// A new, empty guide; returns its id. A blank name is "New Guide".
    pub fn add_guide(&mut self, name: &str) -> u64 {
        let id = self.id();
        let name = match name.trim() {
            "" => "New Guide".to_owned(),
            name => name.to_owned(),
        };
        self.guides.push(Guide {
            id,
            name,
            places: Vec::new(),
        });
        id
    }

    pub fn remove_guide(&mut self, id: u64) {
        self.guides.retain(|guide| guide.id != id);
    }

    /// Adds `place` to the guide, or takes it out if there; returns whether
    /// it is now in it, or `None` with no such guide.
    pub fn toggle_in_guide(&mut self, id: u64, place: Place, now: u64) -> Option<bool> {
        let guide = self.guides.iter_mut().find(|guide| guide.id == id)?;
        Some(toggle(&mut guide.places, place, now))
    }

    /// Keeps directions from `from` to `to`, replacing the same trip.
    pub fn save_route(&mut self, from: Place, to: Place, mode: &str, now: u64) -> u64 {
        self.routes.retain(|route| {
            !(route.from.id == from.id && route.to.id == to.id && route.mode == mode)
        });
        let id = self.id();
        self.routes.insert(
            0,
            SavedRoute {
                id,
                from,
                to,
                mode: mode.to_owned(),
                added: now,
            },
        );
        id
    }

    pub fn remove_route(&mut self, id: u64) {
        self.routes.retain(|route| route.id != id);
    }
}

fn toggle(list: &mut Vec<Stamped>, place: Place, now: u64) -> bool {
    if let Some(index) = list.iter().position(|entry| entry.place.id == place.id) {
        list.remove(index);
        false
    } else {
        list.insert(0, Stamped { place, added: now });
        true
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::Category};

    fn place(id: &str) -> Place {
        Place {
            id: id.into(),
            name: id.into(),
            subtitle: String::new(),
            lat: 37.0,
            lon: -122.0,
            category: Category::Pin,
        }
    }

    #[test]
    fn pins_toggle_and_recently_added_dedupes() {
        let mut library = Library::default();
        assert!(library.toggle_pin(place("a"), 1));
        assert!(library.toggle_save(place("a"), 5));
        assert!(library.toggle_save(place("b"), 3));
        let guide = library.add_guide("  ");
        assert_eq!(library.toggle_in_guide(guide, place("c"), 4), Some(true));
        let ids: Vec<_> = library
            .recently_added(10)
            .iter()
            .map(|entry| entry.place.id.clone())
            .collect();
        assert_eq!(ids, ["a", "c", "b"]);
        assert_eq!(library.guides[0].name, "New Guide");
        assert!(!library.toggle_pin(place("a"), 6));
        assert!(!library.is_pinned("a"));
        assert_eq!(library.toggle_in_guide(guide + 1, place("c"), 4), None);
    }

    #[test]
    fn recents_move_to_front_and_are_capped() {
        let mut library = Library::default();
        for index in 0..(MAX_RECENTS + 5) {
            library.remember(Recent::Place {
                place: place(&index.to_string()),
                at: index as u64,
            });
        }
        library.remember(Recent::Place {
            place: place("10"),
            at: 99,
        });
        library.remember(Recent::Search {
            query: "Coffee".into(),
            near: String::new(),
            at: 100,
        });
        library.remember(Recent::Search {
            query: "coffee".into(),
            near: String::new(),
            at: 101,
        });
        assert_eq!(library.recents.len(), MAX_RECENTS);
        assert!(matches!(
            &library.recents[0],
            Recent::Search { at: 101, .. }
        ));
        assert!(matches!(&library.recents[1], Recent::Place { place, .. } if place.id == "10"));
    }

    #[test]
    fn visits_count_and_routes_replace_the_same_trip() {
        let mut library = Library::default();
        library.visit(place("a"), 1);
        library.visit(place("b"), 2);
        library.visit(place("a"), 3);
        assert_eq!(library.visits[0].count, 2);
        assert_eq!(library.visits.len(), 2);
        let first = library.save_route(place("a"), place("b"), "car", 1);
        let second = library.save_route(place("a"), place("b"), "car", 2);
        library.save_route(place("a"), place("b"), "walk", 3);
        assert_ne!(first, second);
        assert_eq!(library.routes.len(), 2);
        library.remove_route(second);
        assert_eq!(library.routes.len(), 1);
    }

    #[test]
    fn saves_atomically_and_round_trips() -> Result<(), LibraryError> {
        let dir = tempfile::tempdir().map_err(|source| LibraryError::Read {
            path: PathBuf::new(),
            source,
        })?;
        let path = dir.path().join("library.json");
        assert_eq!(Library::load(&path)?, Library::default());
        let mut library = Library::default();
        library.toggle_pin(place("a"), 1);
        library
            .preferences
            .keys
            .insert("stadia-smooth".into(), "secret".into());
        library.save(&path)?;
        assert_eq!(Library::load(&path)?, library);
        assert!(!path.with_extension("json.partial").exists());
        std::fs::write(&path, b"{ not json").map_err(|source| LibraryError::Write {
            path: path.clone(),
            source,
        })?;
        assert!(matches!(
            Library::load(&path),
            Err(LibraryError::Parse { .. })
        ));
        Ok(())
    }
}
