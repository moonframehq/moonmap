use serde::{Deserialize, Serialize};

/// A place on Earth in degrees, WGS 84.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Coordinate {
    pub lat: f64,
    pub lon: f64,
}

impl Coordinate {
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

const EARTH_RADIUS_METERS: f64 = 6_371_008.8;

/// Great-circle distance between two coordinates (haversine).
pub fn distance_meters(a: Coordinate, b: Coordinate) -> f64 {
    let (lat_a, lat_b) = (a.lat.to_radians(), b.lat.to_radians());
    let d_lat = lat_b - lat_a;
    let d_lon = (b.lon - a.lon).to_radians();
    let h = (d_lat / 2.0).sin().powi(2) + lat_a.cos() * lat_b.cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_METERS * h.sqrt().asin()
}

/// The compass bearing from `a` to `b`, in degrees.
pub fn bearing_degrees(a: Coordinate, b: Coordinate) -> f64 {
    let (lat_a, lat_b) = (a.lat.to_radians(), b.lat.to_radians());
    let d_lon = (b.lon - a.lon).to_radians();
    let y = d_lon.sin() * lat_b.cos();
    let x = lat_a.cos() * lat_b.sin() - lat_a.sin() * lat_b.cos() * d_lon.cos();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

/// Where `meters` along `bearing` from `from` ends, on a short trip.
pub fn destination(from: Coordinate, bearing: f64, meters: f64) -> Coordinate {
    let (sin, cos) = bearing.to_radians().sin_cos();
    let angle = meters / EARTH_RADIUS_METERS;
    Coordinate::new(
        from.lat + (angle * cos).to_degrees(),
        from.lon + (angle * sin / from.lat.to_radians().cos().abs().max(0.01)).to_degrees(),
    )
}

/// "18 min", "1 hr 3 min", as Maps writes travel times.
pub fn duration_label(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round().max(1.0) as u64;
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} hr"),
        (hours, minutes) => format!("{hours} hr {minutes} min"),
    }
}

/// "850 m", "12 km", "1.4 km".
pub fn distance_label(meters: f64) -> String {
    if meters < 1000.0 {
        format!("{} m", (meters / 10.0).round() * 10.0)
    } else if meters < 10_000.0 {
        format!("{:.1} km", meters / 1000.0)
    } else {
        format!("{} km", (meters / 1000.0).round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_between_cities_is_close_to_published() {
        let cupertino = Coordinate::new(37.3230, -122.0322);
        let san_francisco = Coordinate::new(37.7749, -122.4194);
        let km = distance_meters(cupertino, san_francisco) / 1000.0;
        assert!((km - 60.5).abs() < 1.5, "{km}");
        assert_eq!(distance_meters(cupertino, cupertino), 0.0);
    }

    #[test]
    fn labels_read_like_maps() {
        assert_eq!(duration_label(18.0 * 60.0), "18 min");
        assert_eq!(duration_label(63.0 * 60.0), "1 hr 3 min");
        assert_eq!(duration_label(120.0 * 60.0), "2 hr");
        assert_eq!(duration_label(5.0), "1 min");
        assert_eq!(distance_label(847.0), "850 m");
        assert_eq!(distance_label(1430.0), "1.4 km");
        assert_eq!(distance_label(24_600.0), "25 km");
    }
}
