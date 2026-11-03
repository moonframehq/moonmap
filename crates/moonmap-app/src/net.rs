//! Requests to the keyless web services, each on its own short-lived
//! thread so the UI and the provider's tick never wait on the network.

use {
    anyhow::{Context, Result},
    moonmap_core::{Coordinate, Drive, Place, Weather},
    std::{sync::mpsc::Sender, time::Duration},
};

const USER_AGENT: &str = concat!(
    "Moonmap/",
    env!("CARGO_PKG_VERSION"),
    " (+https://moonframe.dev)"
);

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
