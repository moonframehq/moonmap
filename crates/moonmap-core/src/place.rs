use {
    crate::Coordinate,
    serde::{Deserialize, Serialize},
};

/// What kind of place something is: it picks the round badge Maps draws
/// beside it in lists and the color of its pin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    #[default]
    Pin,
    Search,
    Hotel,
    Store,
    Food,
    Cafe,
    Park,
    School,
    Airport,
    Home,
    Work,
}

impl Category {
    /// A Moonframe icon name for the badge.
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Pin => "pin",
            Self::Search => "search",
            Self::Hotel => "house",
            Self::Store => "tag",
            Self::Food => "heart",
            Self::Cafe => "coffee",
            Self::Park => "sun",
            Self::School => "book-open",
            Self::Airport => "send",
            Self::Home => "house",
            Self::Work => "building-2",
        }
    }

    /// The badge's fill, from Maps' own category palette.
    pub const fn color(self) -> &'static str {
        match self {
            Self::Pin => "#f0424e",
            Self::Search => "#8e8e93",
            Self::Hotel => "#a45ee5",
            Self::Store => "#f5b300",
            Self::Food => "#ff8a1f",
            Self::Cafe => "#a2754b",
            Self::Park => "#34c759",
            Self::School => "#a2845e",
            Self::Airport => "#0a84ff",
            Self::Home => "#0a84ff",
            Self::Work => "#8e6bd8",
        }
    }

    /// From an OpenStreetMap `category`/`type` pair, as Nominatim gives them.
    pub fn from_osm(class: &str, kind: &str) -> Self {
        match (class, kind) {
            ("tourism", "hotel" | "motel" | "hostel" | "guest_house") => Self::Hotel,
            ("amenity", "restaurant" | "fast_food" | "food_court" | "bar" | "pub") => Self::Food,
            ("amenity", "cafe") => Self::Cafe,
            ("amenity", "school" | "college" | "university" | "kindergarten") => Self::School,
            ("aeroway", _) => Self::Airport,
            ("leisure", "park" | "garden" | "playground" | "golf_course" | "nature_reserve") => {
                Self::Park
            },
            ("shop", _) => Self::Store,
            ("building", "house" | "residential") | ("place", "house") => Self::Home,
            ("office", _) => Self::Work,
            _ => Self::Pin,
        }
    }
}

/// A place the user can find, pin, save, route to and revisit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// Stable across searches: `osm:node/123`, or `geo:<lat>,<lon>`.
    pub id: String,
    pub name: String,
    /// A short address line: "950 Mason St, San Francisco".
    #[serde(default)]
    pub subtitle: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub category: Category,
}

impl Place {
    /// A dropped pin at `at`, identified by its rounded coordinate.
    pub fn dropped(at: Coordinate) -> Self {
        Self {
            id: format!("geo:{:.5},{:.5}", at.lat, at.lon),
            name: "Dropped Pin".into(),
            subtitle: format!("{:.5}, {:.5}", at.lat, at.lon),
            lat: at.lat,
            lon: at.lon,
            category: Category::Pin,
        }
    }

    pub fn coordinate(&self) -> Coordinate {
        Coordinate::new(self.lat, self.lon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osm_classes_map_to_badges() {
        assert_eq!(Category::from_osm("tourism", "hotel"), Category::Hotel);
        assert_eq!(Category::from_osm("shop", "furniture"), Category::Store);
        assert_eq!(
            Category::from_osm("aeroway", "aerodrome"),
            Category::Airport
        );
        assert_eq!(Category::from_osm("highway", "residential"), Category::Pin);
    }

    #[test]
    fn dropped_pins_share_an_id_at_the_same_spot() {
        let a = Place::dropped(Coordinate::new(37.123_451, -122.0));
        let b = Place::dropped(Coordinate::new(37.123_449, -122.0));
        assert_eq!(a.id, b.id);
    }
}
