//! Parsers for the web services Moonmap asks: Nominatim (search), OSRM
//! (driving times) and Open-Meteo (weather and air quality). The app does
//! the requests on its workers; here is only what their answers mean.

use {
    crate::{Category, Place},
    serde::{Deserialize, Serialize},
};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("unexpected answer: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no route: {0}")]
    NoRoute(String),
    #[error("bad coordinate {0:?}")]
    Coordinate(String),
}

#[derive(Deserialize)]
struct Found {
    osm_type: Option<String>,
    osm_id: Option<u64>,
    lat: String,
    lon: String,
    #[serde(default)]
    category: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    address: serde_json::Map<String, serde_json::Value>,
}

/// Places from a Nominatim `format=jsonv2&addressdetails=1` search.
pub fn parse_search(body: &[u8]) -> Result<Vec<Place>, ServiceError> {
    let found: Vec<Found> = serde_json::from_slice(body)?;
    found.into_iter().map(place).collect()
}

fn place(found: Found) -> Result<Place, ServiceError> {
    let number = |text: &str| {
        text.parse::<f64>()
            .map_err(|_| ServiceError::Coordinate(text.to_owned()))
    };
    let (lat, lon) = (number(&found.lat)?, number(&found.lon)?);
    let field = |key: &str| {
        found
            .address
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
    };
    let street = [field("house_number"), field("road")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let town = [
        "city", "town", "village", "suburb", "hamlet", "county", "state",
    ]
    .into_iter()
    .map(field)
    .find(|part| !part.is_empty())
    .unwrap_or_default();
    let parts: Vec<&str> = found.display_name.split(", ").collect();
    let name = if found.name.is_empty() {
        parts.first().copied().unwrap_or("Unnamed place").to_owned()
    } else {
        found.name.clone()
    };
    let mut subtitle = [street.as_str(), town]
        .into_iter()
        .filter(|part| !part.is_empty() && *part != name)
        .collect::<Vec<_>>()
        .join(", ");
    if subtitle.is_empty() {
        subtitle = parts
            .iter()
            .skip(1)
            .take(2)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
    }
    let id = match (found.osm_type, found.osm_id) {
        (Some(kind), Some(id)) => format!("osm:{kind}/{id}"),
        _ => format!("geo:{lat:.5},{lon:.5}"),
    };
    Ok(Place {
        id,
        name,
        subtitle,
        lat,
        lon,
        category: Category::from_osm(&found.category, &found.kind),
    })
}

/// A drive's length, time and the roads it mostly takes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Drive {
    pub seconds: f64,
    pub meters: f64,
    /// OSRM's summary: "CA 85, I 280"; empty when it has none.
    pub roads: String,
}

#[derive(Deserialize)]
struct Routes {
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    routes: Vec<OsrmRoute>,
}

#[derive(Deserialize)]
struct OsrmRoute {
    duration: f64,
    distance: f64,
    #[serde(default)]
    legs: Vec<Leg>,
}

#[derive(Deserialize)]
struct Leg {
    #[serde(default)]
    summary: String,
}

/// The first route of an OSRM `route/v1` answer.
pub fn parse_drive(body: &[u8]) -> Result<Drive, ServiceError> {
    let answer: Routes = serde_json::from_slice(body)?;
    if answer.code != "Ok" {
        return Err(ServiceError::NoRoute(if answer.message.is_empty() {
            answer.code
        } else {
            answer.message
        }));
    }
    let route = answer
        .routes
        .into_iter()
        .next()
        .ok_or_else(|| ServiceError::NoRoute("no routes".into()))?;
    let roads = route
        .legs
        .iter()
        .map(|leg| leg.summary.as_str())
        .filter(|summary| !summary.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Drive {
        seconds: route.duration,
        meters: route.distance,
        roads,
    })
}

/// The weather badge in the map's corner.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Weather {
    pub celsius: f64,
    /// A Moonframe icon for the WMO weather code.
    pub icon: &'static str,
    pub summary: &'static str,
    /// US AQI, when the air-quality service answered.
    pub aqi: Option<u32>,
}

#[derive(Deserialize)]
struct Forecast {
    current: Current,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f64,
    #[serde(default)]
    weather_code: u32,
    #[serde(default = "day")]
    is_day: u8,
}

const fn day() -> u8 {
    1
}

#[derive(Deserialize)]
struct Air {
    current: AirNow,
}

#[derive(Deserialize)]
struct AirNow {
    us_aqi: Option<f64>,
}

/// Open-Meteo's forecast `current`, with its air-quality answer if any.
pub fn parse_weather(forecast: &[u8], air: Option<&[u8]>) -> Result<Weather, ServiceError> {
    let current = serde_json::from_slice::<Forecast>(forecast)?.current;
    let aqi = air
        .and_then(|body| serde_json::from_slice::<Air>(body).ok())
        .and_then(|air| air.current.us_aqi)
        .map(|aqi| aqi.round().max(0.0) as u32);
    let (icon, summary) = match current.weather_code {
        0 | 1 if current.is_day == 0 => ("moon", "Clear"),
        0 => ("sun", "Clear"),
        1 => ("sun", "Mostly Clear"),
        2 | 3 => ("cloud", "Cloudy"),
        45 | 48 => ("cloud", "Fog"),
        51..=67 | 80..=82 => ("cloud", "Rain"),
        71..=77 | 85 | 86 => ("cloud", "Snow"),
        95..=99 => ("zap", "Thunderstorms"),
        _ => ("cloud", "Cloudy"),
    };
    Ok(Weather {
        celsius: current.temperature_2m,
        icon,
        summary,
        aqi,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominatim_results_become_places() -> Result<(), ServiceError> {
        let body = br#"[
          {"osm_type":"node","osm_id":42,"lat":"37.7920","lon":"-122.4100","category":"tourism",
           "type":"hotel","name":"Fairmont San Francisco",
           "display_name":"Fairmont San Francisco, 950, Mason Street, Nob Hill, San Francisco, California, United States",
           "address":{"house_number":"950","road":"Mason Street","city":"San Francisco"}},
          {"lat":"37.3","lon":"-122.0","name":"","display_name":"Cupertino, Santa Clara County, California"}
        ]"#;
        let places = parse_search(body)?;
        assert_eq!(places[0].id, "osm:node/42");
        assert_eq!(places[0].subtitle, "950 Mason Street, San Francisco");
        assert_eq!(places[0].category, Category::Hotel);
        assert_eq!(places[1].name, "Cupertino");
        assert_eq!(places[1].subtitle, "Santa Clara County, California");
        assert!(places[1].id.starts_with("geo:"));
        assert!(matches!(
            parse_search(br#"[{"lat":"north","lon":"0"}]"#),
            Err(ServiceError::Coordinate(_))
        ));
        Ok(())
    }

    #[test]
    fn osrm_routes_and_failures() -> Result<(), ServiceError> {
        let drive = parse_drive(
            br#"{"code":"Ok","routes":[{"duration":1080.5,"distance":18000,"legs":[{"summary":"CA 85, I 280"}]}]}"#,
        )?;
        assert_eq!(drive.roads, "CA 85, I 280");
        assert_eq!(drive.seconds, 1080.5);
        assert!(matches!(
            parse_drive(br#"{"code":"NoRoute","message":"Impossible route"}"#),
            Err(ServiceError::NoRoute(message)) if message == "Impossible route"
        ));
        Ok(())
    }

    #[test]
    fn weather_reads_code_and_optional_air() -> Result<(), ServiceError> {
        let forecast = br#"{"current":{"temperature_2m":18.4,"weather_code":0,"is_day":1}}"#;
        let weather = parse_weather(forecast, Some(br#"{"current":{"us_aqi":15.2}}"#))?;
        assert_eq!((weather.icon, weather.aqi), ("sun", Some(15)));
        let night = br#"{"current":{"temperature_2m":9,"weather_code":1,"is_day":0}}"#;
        let weather = parse_weather(night, Some(b"oops"))?;
        assert_eq!((weather.icon, weather.aqi), ("moon", None));
        Ok(())
    }
}
