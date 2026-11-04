//! What the place sheet shows beyond a place's name: OpenStreetMap's tags,
//! from a Nominatim lookup, and Wikipedia's words and pictures. No service
//! here gives ratings or offers, so neither do these.

use {
    crate::ServiceError,
    serde::{Deserialize, Serialize},
    std::collections::BTreeMap,
};

/// The most pictures a sheet shows.
pub const MOST_PHOTOS: usize = 6;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Details {
    /// "Hotel", "Ferry terminal".
    pub kind: String,
    /// The neighbourhood, town or city: "Nob Hill, San Francisco".
    pub area: String,
    pub phone: String,
    pub website: String,
    /// One line per rule: "Mon–Fri 07:00–22:00".
    pub hours: Vec<String>,
    pub about: String,
    /// The Wikipedia page `about` came from.
    pub source: String,
    /// Files, once downloaded.
    pub photos: Vec<String>,
}

/// What a Nominatim answer knows, and where to read on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub details: Details,
    /// `en:Fairmont San Francisco`.
    pub wikipedia: String,
    pub wikidata: String,
}

#[derive(Deserialize)]
struct Hit {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    extratags: Option<BTreeMap<String, String>>,
    #[serde(default)]
    address: BTreeMap<String, serde_json::Value>,
}

/// The first place of a Nominatim `extratags=1&addressdetails=1` answer.
pub fn parse_found(body: &[u8]) -> Result<Option<Found>, ServiceError> {
    let hits: Vec<Hit> = serde_json::from_slice(body)?;
    let Some(hit) = hits.into_iter().next() else {
        return Ok(None);
    };
    let tags = hit.extratags.unwrap_or_default();
    let tag = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| tags.get(*key))
            .cloned()
            .unwrap_or_default()
    };
    let field = |key: &str| {
        hit.address
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
    };
    let near = ["quarter", "neighbourhood", "suburb"]
        .into_iter()
        .map(field)
        .find(|part| !part.is_empty())
        .unwrap_or_default();
    let town = ["city", "town", "village", "county"]
        .into_iter()
        .map(field)
        .find(|part| !part.is_empty())
        .unwrap_or_default();
    let area = [near, town]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Some(Found {
        details: Details {
            kind: label(&hit.kind),
            area,
            phone: tag(&["phone", "contact:phone"]),
            website: web_only(tag(&["website", "contact:website", "url"])),
            hours: hours(&tag(&["opening_hours"])),
            ..Details::default()
        },
        wikipedia: Some(tag(&["wikipedia"]))
            .filter(|tag| {
                tag.split_once(':')
                    .is_some_and(|(code, _)| is_language(code))
            })
            .unwrap_or_default(),
        wikidata: Some(tag(&["wikidata"]))
            .filter(|id| is_wikidata_id(id))
            .unwrap_or_default(),
    }))
}

// Anyone can edit OpenStreetMap, so its tags are untrusted: they name hosts
// to fetch from and links the system opens. Only what fits gets through.

/// A Wikipedia language code, which becomes part of a host name.
fn is_language(code: &str) -> bool {
    (2..=12).contains(&code.len())
        && code
            .chars()
            .all(|letter| letter.is_ascii_lowercase() || letter == '-')
}

/// `Q123`, which becomes part of a path.
fn is_wikidata_id(id: &str) -> bool {
    id.strip_prefix('Q')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
}

/// `url` if it is a web address, which the system opens in a browser;
/// another scheme could start any app, or open a file.
fn web_only(url: String) -> String {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        url
    } else {
        String::new()
    }
}

/// Whether Wikimedia's image servers serve `url`.
fn is_wikimedia_image(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .is_some_and(|host| matches!(host, "upload.wikimedia.org" | "thumb.wikimedia.org"))
}

/// `ferry_terminal` as a person would write it.
fn label(kind: &str) -> String {
    let words = kind.replace('_', " ");
    // Short ones are initials: `office=it`.
    if words.len() <= 2 {
        return words.to_uppercase();
    }
    let mut letters = words.chars();
    match letters.next() {
        Some(first) if words != "yes" => first.to_uppercase().chain(letters).collect(),
        _ => String::new(),
    }
}

/// OpenStreetMap's `opening_hours`, a rule to a line, with its days spelled.
fn hours(text: &str) -> Vec<String> {
    const DAYS: [(&str, &str); 8] = [
        ("Mo", "Mon"),
        ("Tu", "Tue"),
        ("We", "Wed"),
        ("Th", "Thu"),
        ("Fr", "Fri"),
        ("Sa", "Sat"),
        ("Su", "Sun"),
        ("PH", "Holidays"),
    ];
    text.split(';')
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
        .map(|rule| {
            let spelled = DAYS.iter().fold(rule.to_owned(), |rule, (short, long)| {
                rule.replace(short, long)
            });
            spelled.replace('-', "–")
        })
        .collect()
}

#[derive(Deserialize)]
struct Summary {
    #[serde(default)]
    extract: String,
    #[serde(default)]
    content_urls: Option<Urls>,
}

#[derive(Deserialize)]
struct Urls {
    desktop: Page,
}

#[derive(Deserialize)]
struct Page {
    page: String,
}

/// The words of a Wikipedia `page/summary`, and its page.
pub fn parse_summary(body: &[u8]) -> Result<(String, String), ServiceError> {
    let summary: Summary = serde_json::from_slice(body)?;
    let page = summary
        .content_urls
        .map(|urls| urls.desktop.page)
        .unwrap_or_default();
    Ok((summary.extract, web_only(page)))
}

#[derive(Deserialize)]
struct Media {
    #[serde(default)]
    items: Vec<MediaItem>,
}

#[derive(Deserialize)]
struct MediaItem {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "showInGallery")]
    gallery: bool,
    #[serde(default)]
    srcset: Vec<Source>,
}

#[derive(Deserialize)]
struct Source {
    src: String,
}

/// The photographs of a Wikipedia `page/media-list`, sharpest copy of each:
/// no logos, maps or flags, which are drawings and files of other kinds.
pub fn parse_media(body: &[u8]) -> Result<Vec<String>, ServiceError> {
    let media: Media = serde_json::from_slice(body)?;
    Ok(media
        .items
        .into_iter()
        .filter(|item| item.kind == "image" && item.gallery)
        .filter(|item| {
            let title = item.title.to_lowercase();
            let photo = title.ends_with(".jpg") || title.ends_with(".jpeg");
            photo
                && !["logo", "map", "flag", "seal"]
                    .iter()
                    .any(|word| title.contains(word))
        })
        .filter_map(|item| item.srcset.into_iter().last())
        .map(|source| match source.src.strip_prefix("//") {
            Some(rest) => format!("https://{rest}"),
            None => source.src,
        })
        .filter(|url| is_wikimedia_image(url))
        .take(MOST_PHOTOS)
        .collect())
}

/// The English Wikipedia title on a Wikidata `EntityData` answer for `id`.
pub fn parse_wikidata(body: &[u8], id: &str) -> Result<Option<String>, ServiceError> {
    let data: serde_json::Value = serde_json::from_slice(body)?;
    Ok(data["entities"][id]["sitelinks"]["enwiki"]["title"]
        .as_str()
        .map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominatim_tags_become_details() -> Result<(), ServiceError> {
        let body = br#"[{"type":"ferry_terminal","address":{"quarter":"Nob Hill","city":"San Francisco"},
          "extratags":{"contact:phone":"+1 415","website":"https://x","wikipedia":"en:Ferry Building",
          "opening_hours":"Mo-Fr 07:00-22:00; Sa 08:00-18:00"}}]"#;
        let found = parse_found(body)?.ok_or(ServiceError::NoRoute("none".into()))?;
        assert_eq!(found.details.kind, "Ferry terminal");
        assert_eq!(found.details.area, "Nob Hill, San Francisco");
        assert_eq!(found.details.phone, "+1 415");
        assert_eq!(
            found.details.hours,
            ["Mon–Fri 07:00–22:00", "Sat 08:00–18:00"]
        );
        assert_eq!(found.wikipedia, "en:Ferry Building");
        assert_eq!(parse_found(b"[]")?, None);
        Ok(())
    }

    #[test]
    fn untrusted_tags_cannot_name_hosts_paths_or_schemes() -> Result<(), ServiceError> {
        let body = br#"[{"type":"cafe","extratags":{
          "wikipedia":"evil.example/#:Page","wikidata":"Q1/../../x","website":"file:///etc/passwd"}}]"#;
        let found = parse_found(body)?.ok_or(ServiceError::NoRoute("none".into()))?;
        assert_eq!(found.wikipedia, "");
        assert_eq!(found.wikidata, "");
        assert_eq!(found.details.website, "");
        for scheme in ["javascript:alert(1)", "moonmap://x", "ftp://x"] {
            assert_eq!(web_only(scheme.to_owned()), "");
        }
        assert_eq!(
            web_only("HTTPS://a.example".to_owned()),
            "HTTPS://a.example"
        );
        assert!(is_language("zh-classical") && is_wikidata_id("Q1408117"));
        Ok(())
    }

    #[test]
    fn a_tag_without_a_kind_shows_none() {
        assert_eq!(label("yes"), "");
        assert_eq!(label("it"), "IT");
    }

    #[test]
    fn media_keeps_photographs_at_their_sharpest() -> Result<(), ServiceError> {
        let body = br#"{"items":[
          {"type":"image","title":"File:Fairmont_Logo.jpg","showInGallery":true,"srcset":[{"src":"//thumb.wikimedia.org/logo.jpg"}]},
          {"type":"image","title":"File:Plan.svg","showInGallery":true,"srcset":[{"src":"//thumb.wikimedia.org/plan.svg"}]},
          {"type":"image","title":"File:Hotel.jpg","showInGallery":false,"srcset":[{"src":"//thumb.wikimedia.org/hidden.jpg"}]},
          {"type":"image","title":"File:Lobby.JPG","showInGallery":true,
           "srcset":[{"src":"//w/1x.jpg"},{"src":"//upload.wikimedia.org/2x.jpg"}]},
          {"type":"image","title":"File:Elsewhere.jpg","showInGallery":true,
           "srcset":[{"src":"https://upload.wikimedia.org.evil.example/x.jpg"}]}
        ]}"#;
        assert_eq!(parse_media(body)?, ["https://upload.wikimedia.org/2x.jpg"]);
        Ok(())
    }

    #[test]
    fn summaries_and_wikidata_titles() -> Result<(), ServiceError> {
        let body =
            br#"{"extract":"A hotel.","content_urls":{"desktop":{"page":"https://w/page"}}}"#;
        assert_eq!(
            parse_summary(body)?,
            ("A hotel.".into(), "https://w/page".into())
        );
        let data = br#"{"entities":{"Q1":{"sitelinks":{"enwiki":{"title":"Ferry Building"}}}}}"#;
        assert_eq!(parse_wikidata(data, "Q1")?, Some("Ferry Building".into()));
        assert_eq!(parse_wikidata(data, "Q2")?, None);
        Ok(())
    }
}
