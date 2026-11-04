//! Street-level panoramas: what Mapillary and Panoramax say about the 360°
//! shots near a spot. The app does the requests; here is what answers mean.

use {
    crate::{Coordinate, ServiceError, distance_meters},
    serde::{Deserialize, Serialize},
};

/// One 360° photo, stored equirectangular.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Shot {
    pub id: String,
    pub lat: f64,
    pub lon: f64,
    /// The compass bearing at the middle of the image, in degrees.
    pub heading: f64,
    /// A smaller copy, which shows first.
    pub url: String,
    /// The sharpest copy, once it arrives.
    pub hd: String,
    /// The source and license, which must be shown with the image.
    pub credit: String,
}

impl Shot {
    pub fn coordinate(&self) -> Coordinate {
        Coordinate::new(self.lat, self.lon)
    }
}

/// The closest of `shots` to `at`, other than the one called `skip`.
pub fn nearest(shots: Vec<Shot>, at: Coordinate, skip: Option<&str>) -> Option<Shot> {
    shots
        .into_iter()
        .filter(|shot| Some(shot.id.as_str()) != skip)
        .min_by(|a, b| {
            distance_meters(a.coordinate(), at).total_cmp(&distance_meters(b.coordinate(), at))
        })
}

#[derive(Deserialize)]
struct Items {
    #[serde(default)]
    features: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    id: String,
    geometry: Point,
    properties: Properties,
    assets: Assets,
    #[serde(default)]
    links: Vec<Link>,
}

#[derive(Deserialize)]
struct Point {
    coordinates: [f64; 2],
}

#[derive(Deserialize)]
struct Properties {
    #[serde(rename = "view:azimuth", default)]
    azimuth: f64,
    #[serde(rename = "pers:interior_orientation", default)]
    orientation: Orientation,
}

#[derive(Default, Deserialize)]
struct Orientation {
    field_of_view: Option<f64>,
}

#[derive(Deserialize)]
struct Assets {
    sd: Asset,
    hd: Asset,
}

#[derive(Deserialize)]
struct Asset {
    href: String,
}

#[derive(Deserialize)]
struct Link {
    rel: String,
    #[serde(default)]
    title: String,
}

/// The 360° shots of a Panoramax `/api/search` answer.
pub fn parse_panoramax(body: &[u8]) -> Result<Vec<Shot>, ServiceError> {
    let items: Items = serde_json::from_slice(body)?;
    Ok(items
        .features
        .into_iter()
        .filter(|item| item.properties.orientation.field_of_view == Some(360.0))
        .map(|item| {
            let license = item
                .links
                .iter()
                .find(|link| link.rel == "license")
                .and_then(|link| link.title.strip_prefix("License for this object ("))
                .map(|title| title.trim_end_matches(')'));
            let [lon, lat] = item.geometry.coordinates;
            Shot {
                id: item.id,
                lat,
                lon,
                heading: item.properties.azimuth,
                url: item.assets.sd.href,
                hd: item.assets.hd.href,
                credit: match license {
                    Some(license) => format!("Panoramax contributors, {license}"),
                    None => "Panoramax contributors".into(),
                },
            }
        })
        .collect())
}

#[derive(Deserialize)]
struct Images {
    #[serde(default)]
    data: Vec<Image>,
}

#[derive(Deserialize)]
struct Image {
    id: String,
    computed_geometry: Option<Point>,
    geometry: Option<Point>,
    computed_compass_angle: Option<f64>,
    compass_angle: Option<f64>,
    #[serde(default)]
    is_pano: bool,
    thumb_2048_url: Option<String>,
    thumb_original_url: Option<String>,
}

/// The 360° shots of a Mapillary `/images` answer.
pub fn parse_mapillary(body: &[u8]) -> Result<Vec<Shot>, ServiceError> {
    let images: Images = serde_json::from_slice(body)?;
    Ok(images
        .data
        .into_iter()
        .filter(|image| image.is_pano)
        .filter_map(|image| {
            let [lon, lat] = image.computed_geometry.or(image.geometry)?.coordinates;
            let url = image.thumb_2048_url?;
            Some(Shot {
                id: image.id,
                lat,
                lon,
                heading: image
                    .computed_compass_angle
                    .or(image.compass_angle)
                    .unwrap_or_default(),
                hd: image.thumb_original_url.unwrap_or_else(|| url.clone()),
                url,
                credit: "Mapillary, CC BY-SA 4.0".into(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANORAMAX: &[u8] = br#"{"features":[
      {"id":"a","geometry":{"type":"Point","coordinates":[2.3,48.8]},
       "properties":{"view:azimuth":107,"pers:interior_orientation":{"field_of_view":360}},
       "assets":{"sd":{"href":"https://x/sd.jpg"},"hd":{"href":"https://x/hd.jpg"}},
       "links":[{"rel":"license","title":"License for this object (CC-BY-SA-4.0)"}]},
      {"id":"flat","geometry":{"type":"Point","coordinates":[2.3,48.8]},
       "properties":{"view:azimuth":10,"pers:interior_orientation":{"field_of_view":70}},
       "assets":{"sd":{"href":"https://x/f.jpg"},"hd":{"href":"https://x/fh.jpg"}}}
    ]}"#;

    #[test]
    fn panoramax_keeps_only_panoramas() -> Result<(), ServiceError> {
        let shots = parse_panoramax(PANORAMAX)?;
        assert_eq!(shots.len(), 1);
        assert_eq!((shots[0].lat, shots[0].heading), (48.8, 107.0));
        assert_eq!(shots[0].credit, "Panoramax contributors, CC-BY-SA-4.0");
        Ok(())
    }

    #[test]
    fn mapillary_prefers_computed_values_and_keeps_only_panoramas() -> Result<(), ServiceError> {
        let body = br#"{"data":[
          {"id":"1","is_pano":true,"computed_geometry":{"coordinates":[1,2]},
           "geometry":{"coordinates":[9,9]},"computed_compass_angle":30,"compass_angle":99,
           "thumb_2048_url":"https://m/2048","thumb_original_url":"https://m/full"},
          {"id":"2","is_pano":false,"geometry":{"coordinates":[1,2]},"thumb_2048_url":"https://m/f"}
        ]}"#;
        let shots = parse_mapillary(body)?;
        assert_eq!(shots.len(), 1);
        assert_eq!((shots[0].lon, shots[0].heading), (1.0, 30.0));
        assert_eq!(shots[0].hd, "https://m/full");
        Ok(())
    }

    #[test]
    fn the_nearest_shot_skips_the_one_being_left() -> Result<(), ServiceError> {
        let mut far = parse_panoramax(PANORAMAX)?.remove(0);
        let mut near = far.clone();
        near.id = "near".into();
        near.lat = 48.8001;
        far.lat = 48.9;
        let at = Coordinate::new(48.8, 2.3);
        let shots = vec![far, near];
        assert_eq!(
            nearest(shots.clone(), at, None).map(|s| s.id),
            Some("near".into())
        );
        assert_eq!(
            nearest(shots, at, Some("near")).map(|s| s.id),
            Some("a".into())
        );
        Ok(())
    }
}
