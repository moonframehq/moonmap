//! Moonmap's backend-neutral data: places, the user's library, the tile
//! styles the map can show, and parsers for the web services that answer
//! searches, directions and weather. No UI, network or executor here.

mod catalog;
mod geo;
mod library;
mod place;
mod services;

pub use {
    catalog::{STYLES, TileStyle, style},
    geo::{Coordinate, distance_label, distance_meters, duration_label},
    library::{Guide, Library, LibraryError, Preferences, Recent, SavedRoute, Stamped, Visit},
    place::{Category, Place},
    services::{Drive, ServiceError, Weather, parse_drive, parse_search, parse_weather},
};
