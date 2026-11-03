use serde::Serialize;

/// A map style the user can switch to: one of Moonframe's `ui.map`
/// providers, described for the Map Styles picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TileStyle {
    /// Moonframe's `MapProvider` token.
    pub id: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    /// Credit the provider's terms ask the map to show.
    pub attribution: &'static str,
    /// Whether tiles need the user's API key.
    pub key: bool,
    /// Where to get a key, or read the terms.
    pub url: &'static str,
    /// Drawn for a dark appearance.
    pub dark: bool,
    /// A representative swatch for the picker card: land, then roads.
    pub swatch: [&'static str; 2],
}

/// The styles in the order the picker shows them. The first is the default.
///
/// Moonframe's `carto-*` providers are left out: CARTO's basemaps now
/// answer every keyless request with an "API key required" tile, and
/// `ui.map` has no key slot for them.
pub const STYLES: &[TileStyle] = &[
    TileStyle {
        id: "esri-streets",
        title: "Explore",
        detail: "Esri World Street Map · no key",
        attribution: "Tiles © Esri",
        key: false,
        url: "https://www.esri.com/en-us/legal/terms/full-master-agreement",
        dark: false,
        swatch: ["#f6f3ec", "#fbe28c"],
    },
    TileStyle {
        id: "openstreetmap",
        title: "OpenStreetMap",
        detail: "The standard OSM rendering",
        attribution: "© OpenStreetMap contributors",
        key: false,
        url: "https://www.openstreetmap.org/copyright",
        dark: false,
        swatch: ["#f2efe9", "#e892a2"],
    },
    TileStyle {
        id: "stadia-smooth",
        title: "Smooth",
        detail: "Stadia Alidade Smooth · API key",
        attribution: "© Stadia Maps © OpenMapTiles © OpenStreetMap contributors",
        key: true,
        url: "https://client.stadiamaps.com/signup/",
        dark: false,
        swatch: ["#f4f4f2", "#d5dde4"],
    },
    TileStyle {
        id: "stadia-smooth-dark",
        title: "Smooth Dark",
        detail: "Stadia Alidade Smooth Dark · API key",
        attribution: "© Stadia Maps © OpenMapTiles © OpenStreetMap contributors",
        key: true,
        url: "https://client.stadiamaps.com/signup/",
        dark: true,
        swatch: ["#202326", "#3a4047"],
    },
    TileStyle {
        id: "mapbox-streets",
        title: "Mapbox",
        detail: "Mapbox Streets · access token",
        attribution: "© Mapbox © OpenStreetMap contributors",
        key: true,
        url: "https://account.mapbox.com/access-tokens/",
        dark: false,
        swatch: ["#f2f0eb", "#fde7a5"],
    },
    TileStyle {
        id: "maptiler-streets",
        title: "MapTiler",
        detail: "MapTiler Streets · API key",
        attribution: "© MapTiler © OpenStreetMap contributors",
        key: true,
        url: "https://cloud.maptiler.com/account/keys/",
        dark: false,
        swatch: ["#f8f4ee", "#ffd37a"],
    },
    TileStyle {
        id: "thunderforest-transport",
        title: "Transit",
        detail: "Thunderforest Transport · API key",
        attribution: "Maps © Thunderforest, Data © OpenStreetMap contributors",
        key: true,
        url: "https://manage.thunderforest.com/users/sign_up",
        dark: false,
        swatch: ["#f1f3f1", "#6a8fd8"],
    },
];

/// The style named `id`, or the default for an unknown one.
pub fn style(id: &str) -> &'static TileStyle {
    STYLES
        .iter()
        .find(|style| style.id == id)
        .unwrap_or(&STYLES[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_unknown_falls_back() {
        for (index, style) in STYLES.iter().enumerate() {
            assert!(STYLES[index + 1..].iter().all(|other| other.id != style.id));
            assert!(style.url.starts_with("https://"));
        }
        assert_eq!(style("nope").id, "esri-streets");
        assert_eq!(style("carto-dark").id, "esri-streets");
        assert!(!STYLES[0].key);
    }
}
