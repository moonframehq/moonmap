//! Requests to the keyless web services, each on its own short-lived
//! thread so the UI and the provider's tick never wait on the network.

use {
    crate::street_view::Panorama,
    anyhow::{Context, Result},
    moonmap_core::{
        Coordinate, Details, Drive, Place, Shot, Weather, destination, nearest, parse_found,
        parse_mapillary, parse_media, parse_panoramax, parse_summary, parse_wikidata,
    },
    std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash as _, Hasher as _},
        path::{Path, PathBuf},
        sync::mpsc::Sender,
        time::Duration,
    },
};

const USER_AGENT: &str = concat!(
    "Moonmap/",
    env!("CARGO_PKG_VERSION"),
    " (+https://moonframe.dev)"
);

const PANORAMA_TIMEOUT: Duration = Duration::from_secs(60);
const PANORAMA_BYTES: u64 = 64 * 1024 * 1024;

/// What a worker found, back to the provider's tick.
pub enum Answer {
    Search {
        serial: u64,
        places: Result<Vec<Place>, String>,
    },
    Drive {
        place: String,
        drive: Result<Drive, String>,
    },
    Location(crate::location::Located),
    /// What the sheet shows about the place with this id.
    Details {
        place: String,
        details: Details,
        /// Whether this is all of it; more may follow otherwise.
        done: bool,
    },
    /// The nearest panorama to `asked`, if there is any.
    Street {
        asked: Coordinate,
        shot: Option<Shot>,
    },
    Weather {
        at: Coordinate,
        weather: Result<Weather, String>,
    },
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

fn get(url: &str, query: &[(&str, String)]) -> Result<Vec<u8>> {
    let mut request = agent().get(url);
    for (key, value) in query {
        request = request.query(*key, value);
    }
    let mut response = request
        .call()
        .with_context(|| format!("{url} did not answer"))?;
    Ok(response.body_mut().read_to_vec()?)
}

fn spawn(answers: &Sender<Answer>, work: impl FnOnce() -> Answer + Send + 'static) {
    let answers = answers.clone();
    std::thread::spawn(move || {
        // A closed channel means the provider stopped; nobody is waiting.
        let _ = answers.send(work());
    });
}

/// Nominatim's word for a category people type: "coffee" finds cafés.
fn synonym(query: &str) -> Option<&'static str> {
    Some(match query.trim().to_lowercase().as_str() {
        "coffee" | "coffee shop" | "café" => "cafe",
        "gas" | "gas station" | "petrol" => "fuel",
        "food" | "restaurants" => "restaurant",
        "groceries" | "grocery" | "grocery store" => "supermarket",
        "hotels" | "lodging" => "hotel",
        "doctor" | "hospital" | "er" => "hospital",
        "parking" | "park and ride" => "parking",
        _ => return None,
    })
}

fn nominatim(query: &str, viewbox: Option<&str>) -> Result<Vec<Place>> {
    let mut parameters = vec![
        ("q", query.to_owned()),
        ("format", "jsonv2".into()),
        ("addressdetails", "1".into()),
        ("limit", "12".into()),
    ];
    if let Some(viewbox) = viewbox {
        parameters.push(("viewbox", viewbox.to_owned()));
        parameters.push(("bounded", "1".into()));
    }
    let body = get("https://nominatim.openstreetmap.org/search", &parameters)?;
    Ok(moonmap_core::parse_search(&body)?)
}

/// Nominatim near `near` first (a box about 50 km across, nearest first),
/// then a category word for it, then anywhere: "San Francisco" still finds
/// the city from Cupertino. At most three requests, a second apart, as
/// Nominatim's usage policy asks.
pub fn search(answers: &Sender<Answer>, serial: u64, query: String, near: Coordinate) {
    spawn(answers, move || {
        let viewbox = format!(
            "{},{},{},{}",
            near.lon - 0.3,
            near.lat + 0.25,
            near.lon + 0.3,
            near.lat - 0.25
        );
        let places = (|| {
            let mut places = nominatim(&query, Some(&viewbox))?;
            if places.is_empty()
                && let Some(word) = synonym(&query)
            {
                std::thread::sleep(Duration::from_secs(1));
                places = nominatim(word, Some(&viewbox))?;
            }
            if places.is_empty() {
                std::thread::sleep(Duration::from_secs(1));
                return nominatim(&query, None);
            }
            places.sort_by(|a, b| {
                moonmap_core::distance_meters(near, a.coordinate())
                    .total_cmp(&moonmap_core::distance_meters(near, b.coordinate()))
            });
            Ok(places)
        })()
        .map_err(|error: anyhow::Error| format!("{error:#}"));
        Answer::Search { serial, places }
    });
}

/// A drive from `from` to `place`, along OpenStreetMap's car router.
pub fn drive(answers: &Sender<Answer>, from: Coordinate, place: &Place) {
    let (id, to) = (place.id.clone(), place.coordinate());
    spawn(answers, move || {
        let url = format!(
            "https://routing.openstreetmap.de/routed-car/route/v1/driving/{},{};{},{}",
            from.lon, from.lat, to.lon, to.lat
        );
        let drive = get(&url, &[("overview", "false".into())])
            .and_then(|body| Ok(moonmap_core::parse_drive(&body)?))
            .map_err(|error| format!("{error:#}"));
        Answer::Drive { place: id, drive }
    });
}

/// Open-Meteo's current weather and US AQI at `at`.
/// Gathers what the sheet shows about `place`, on a thread of its own:
/// OpenStreetMap's tags, then Wikipedia's words and pictures, saved in
/// `images`. Answers as it goes, the words first and then each picture, so
/// the sheet fills in. A service that fails only leaves its part out.
pub fn details(answers: &Sender<Answer>, place: Place, images: PathBuf) {
    let progress = answers.clone();
    spawn(answers, move || {
        let id = place.id.clone();
        let tell = |details: &Details| {
            let _ = progress.send(Answer::Details {
                place: id.clone(),
                details: details.clone(),
                done: false,
            });
        };
        let details = gather(&place, &images, &tell).unwrap_or_default();
        Answer::Details {
            place: place.id,
            details,
            done: true,
        }
    });
}

fn gather(place: &Place, images: &Path, tell: &dyn Fn(&Details)) -> Option<Details> {
    let found = parse_found(&look_up(place).ok()?).ok()??;
    let mut details = found.details;
    tell(&details);
    let page = wikipedia_page(&found.wikipedia, &found.wikidata);
    if let Some((language, title)) = page {
        let api = format!("https://{language}.wikipedia.org/api/rest_v1/page");
        let title = title.replace(' ', "_");
        let encoded = url::form_urlencoded::byte_serialize(title.as_bytes()).collect::<String>();
        if let Some((about, source)) = get(&format!("{api}/summary/{encoded}"), &[])
            .ok()
            .and_then(|body| parse_summary(&body).ok())
        {
            details.about = about;
            details.source = source;
            tell(&details);
        }
        let photos = get(&format!("{api}/media-list/{encoded}"), &[])
            .ok()
            .and_then(|body| parse_media(&body).ok())
            .unwrap_or_default();
        for url in &photos {
            if let Ok(file) = save_photo(url, images) {
                details.photos.push(file);
                tell(&details);
            }
        }
    }
    Some(details)
}

/// Nominatim's place, by its id when OpenStreetMap gave one.
fn look_up(place: &Place) -> Result<Vec<u8>> {
    let extras = [
        ("format", "jsonv2".to_owned()),
        ("extratags", "1".to_owned()),
        ("addressdetails", "1".to_owned()),
    ];
    let id = place.id.strip_prefix("osm:").and_then(|id| {
        let (kind, number) = id.split_once('/')?;
        Some(format!(
            "{}{number}",
            kind.chars().next()?.to_ascii_uppercase()
        ))
    });
    match id {
        Some(id) => {
            let mut query = extras.to_vec();
            query.push(("osm_ids", id));
            get("https://nominatim.openstreetmap.org/lookup", &query)
        },
        None => {
            let mut query = extras.to_vec();
            query.push(("q", format!("{} {}", place.name, place.subtitle)));
            query.push(("limit", "1".to_owned()));
            get("https://nominatim.openstreetmap.org/search", &query)
        },
    }
}

/// The language and title of the place's Wikipedia page, through Wikidata
/// when OpenStreetMap only names that.
fn wikipedia_page(tag: &str, wikidata: &str) -> Option<(String, String)> {
    if let Some((language, title)) = tag.split_once(':') {
        return Some((language.to_owned(), title.to_owned()));
    }
    if wikidata.is_empty() {
        return None;
    }
    let url = format!("https://www.wikidata.org/wiki/Special:EntityData/{wikidata}.json");
    let title = parse_wikidata(&get(&url, &[]).ok()?, wikidata).ok()??;
    Some(("en".to_owned(), title))
}

/// Downloads a picture into `images`, once, and says where it is.
fn save_photo(url: &str, images: &Path) -> Result<String> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let file = images.join(format!("photo-{:016x}.jpg", hasher.finish()));
    if !file.exists() {
        std::fs::create_dir_all(images)?;
        std::fs::write(&file, get(url, &[])?)?;
    }
    Ok(file.to_string_lossy().into_owned())
}

/// Finds the panorama for Look Around near `at`, on a thread of its own.
pub fn street(answers: &Sender<Answer>, at: Coordinate, radius: f64, token: String) {
    spawn(answers, move || Answer::Street {
        asked: at,
        shot: shot(at, radius, &token, None),
    });
}

/// The nearest panorama within `radius` meters of `at`, other than `skip`:
/// from Mapillary with a token, which covers more, else Panoramax. Blocks.
pub fn shot(at: Coordinate, radius: f64, token: &str, skip: Option<&str>) -> Option<Shot> {
    let mapillary = (!token.is_empty())
        .then(|| mapillary(at, radius, token))
        .and_then(|shots| nearest(shots, at, skip));
    mapillary.or_else(|| nearest(panoramax(at, radius), at, skip))
}

fn panoramax(at: Coordinate, radius: f64) -> Vec<Shot> {
    let query = [
        ("place_position", format!("{:.6},{:.6}", at.lon, at.lat)),
        ("place_distance", format!("0-{radius:.0}")),
        ("limit", "30".to_owned()),
    ];
    get("https://api.panoramax.xyz/api/search", &query)
        .ok()
        .and_then(|body| parse_panoramax(&body).ok())
        .unwrap_or_default()
}

fn mapillary(at: Coordinate, radius: f64, token: &str) -> Vec<Shot> {
    let north = destination(at, 0.0, radius).lat - at.lat;
    let east = destination(at, 90.0, radius).lon - at.lon;
    let bbox = format!(
        "{:.6},{:.6},{:.6},{:.6}",
        at.lon - east,
        at.lat - north,
        at.lon + east,
        at.lat + north
    );
    let fields = "id,computed_geometry,geometry,computed_compass_angle,compass_angle,is_pano,\
                  thumb_2048_url,thumb_original_url";
    let query = [
        ("access_token", token.to_owned()),
        ("fields", fields.to_owned()),
        ("bbox", bbox),
        ("is_pano", "true".to_owned()),
        ("limit", "30".to_owned()),
    ];
    get("https://graph.mapillary.com/images", &query)
        .ok()
        .and_then(|body| parse_mapillary(&body).ok())
        .unwrap_or_default()
}

/// Downloads and decodes a panorama. Blocks, for a while on the sharpest.
pub fn panorama(url: &str) -> Result<Panorama> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(PANORAMA_TIMEOUT))
        .user_agent(USER_AGENT)
        .build()
        .into();
    let body = agent
        .get(url)
        .call()
        .with_context(|| format!("{url} did not answer"))?
        .body_mut()
        .with_config()
        .limit(PANORAMA_BYTES)
        .read_to_vec()?;
    let image = image::load_from_memory_with_format(&body, image::ImageFormat::Jpeg)?.to_rgb8();
    Ok(Panorama {
        width: image.width(),
        height: image.height(),
        pixels: image.into_raw(),
    })
}

pub fn weather(answers: &Sender<Answer>, at: Coordinate) {
    spawn(answers, move || {
        let here = [
            ("latitude", format!("{:.3}", at.lat)),
            ("longitude", format!("{:.3}", at.lon)),
        ];
        let mut forecast_query = here.to_vec();
        forecast_query.push(("current", "temperature_2m,weather_code,is_day".into()));
        let mut air_query = here.to_vec();
        air_query.push(("current", "us_aqi".into()));
        let weather = get("https://api.open-meteo.com/v1/forecast", &forecast_query)
            .and_then(|forecast| {
                // Air quality is a nicety: the badge shows without it.
                let air = get(
                    "https://air-quality-api.open-meteo.com/v1/air-quality",
                    &air_query,
                )
                .ok();
                Ok(moonmap_core::parse_weather(&forecast, air.as_deref())?)
            })
            .map_err(|error| format!("{error:#}"));
        Answer::Weather { at, weather }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn category_words_have_nominatim_synonyms() {
        assert_eq!(super::synonym(" Coffee "), Some("cafe"));
        assert_eq!(super::synonym("gas station"), Some("fuel"));
        assert_eq!(super::synonym("Blue Bottle"), None);
    }
}
