use std::collections::HashMap;

use axum::{Json, http::StatusCode};
use reqwest::Url;
use serde::{Deserialize, Serialize};

const MAX_LEADS: usize = 200;
const MAX_FIELD_BYTES: usize = 500;
const DEMO: &str = include_str!("demo-leads.csv");

pub(crate) type ApiError = (StatusCode, Json<ErrorBody>);

#[derive(Debug, Serialize)]
pub(crate) struct ErrorBody {
    message: String,
}

pub(crate) fn invalid(message: impl Into<String>) -> ApiError {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(ErrorBody {
            message: message.into(),
        }),
    )
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lead {
    pub(crate) name: String,
    pub(crate) category: String,
    #[serde(default)]
    pub(crate) phone: String,
    #[serde(default)]
    pub(crate) address: String,
    #[serde(default)]
    pub(crate) hours: String,
    #[serde(default)]
    pub(crate) website: String,
    #[serde(default)]
    pub(crate) services: String,
}

impl Lead {
    pub(crate) fn checked(mut self) -> Result<Self, ApiError> {
        for field in [
            &mut self.name,
            &mut self.category,
            &mut self.phone,
            &mut self.address,
            &mut self.hours,
            &mut self.website,
            &mut self.services,
        ] {
            *field = field.trim().to_owned();
            if field.len() > MAX_FIELD_BYTES {
                return Err(invalid("Each lead field must be 500 bytes or less."));
            }
        }
        if self.name.is_empty() || self.category.is_empty() {
            return Err(invalid("Each lead needs a name and category."));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Source {
    Fixture,
    Operator,
    Osm { reference: String },
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WebsiteSignal {
    NoWebsiteListed,
    SocialOrDirectory,
    WebsiteListed,
    ReviewNeeded,
}

#[derive(Serialize)]
pub(crate) struct Candidate {
    lead: Lead,
    source: Source,
    signal: WebsiteSignal,
    score: u8,
}

fn has_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn website_signal(raw: &str) -> WebsiteSignal {
    let mut saw_profile = false;
    let mut saw_unknown = false;
    for site in raw
        .split(';')
        .map(str::trim)
        .filter(|site| !site.is_empty())
    {
        let address = if site.contains("://") || site.starts_with("//") {
            site.to_owned()
        } else {
            format!("https://{site}")
        };
        let url = Url::parse(&address).ok().filter(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.username().is_empty()
                && url.password().is_none()
        });
        let Some(host) = url
            .as_ref()
            .and_then(Url::domain)
            .map(|host| host.trim_end_matches('.'))
        else {
            saw_unknown = true;
            continue;
        };
        if !host.contains('.') {
            saw_unknown = true;
        } else if [
            "facebook.com",
            "fb.me",
            "instagram.com",
            "linktr.ee",
            "yelp.com",
            "yellowpages.com",
            "linkedin.com",
            "google.com",
            "maps.app.goo.gl",
        ]
        .iter()
        .any(|domain| has_domain(host, domain))
        {
            saw_profile = true;
        } else {
            return WebsiteSignal::WebsiteListed;
        }
    }
    match (saw_unknown, saw_profile) {
        (true, _) => WebsiteSignal::ReviewNeeded,
        (_, true) => WebsiteSignal::SocialOrDirectory,
        _ => WebsiteSignal::NoWebsiteListed,
    }
}

fn candidate(lead: Lead, source: Source) -> Candidate {
    let signal = website_signal(&lead.website);
    let score = 3 * u8::from(!lead.phone.is_empty())
        + 2 * u8::from(!lead.address.is_empty())
        + u8::from(!lead.hours.is_empty())
        + u8::from(signal == WebsiteSignal::SocialOrDirectory);
    Candidate {
        lead,
        source,
        signal,
        score,
    }
}

fn csv_leads(input: &str, source: Source) -> Result<Vec<Candidate>, ApiError> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(input.trim_start_matches('\u{feff}').as_bytes());
    let headers = reader
        .headers()
        .map_err(|_| invalid("The CSV header could not be read."))?;
    if !["name", "category"]
        .iter()
        .all(|column| headers.iter().any(|header| header == *column))
    {
        return Err(invalid("The CSV header must contain name and category."));
    }
    reader
        .deserialize::<Lead>()
        .enumerate()
        .map(|(index, row)| {
            if index >= MAX_LEADS {
                return Err(invalid("Import up to 200 leads at a time."));
            }
            let row = row.map_err(|error| {
                let line = error
                    .position()
                    .map_or(index as u64 + 2, csv::Position::line);
                invalid(format!(
                    "Check the CSV columns and values near line {line}."
                ))
            })?;
            Ok(candidate(row.checked()?, source.clone()))
        })
        .collect()
}

#[derive(Deserialize)]
struct OsmExport {
    elements: Vec<OsmElement>,
}

#[derive(Deserialize)]
struct OsmElement {
    id: u64,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    tags: HashMap<String, String>,
}

fn osm_leads(input: &str) -> Result<Vec<Candidate>, ApiError> {
    let export: OsmExport = serde_json::from_str(input)
        .map_err(|_| invalid("Paste an Overpass JSON export with an elements array."))?;
    if export.elements.len() > MAX_LEADS {
        return Err(invalid("Import an export with up to 200 elements."));
    }
    export
        .elements
        .into_iter()
        .filter_map(|element| {
            if !matches!(element.kind.as_str(), "node" | "way" | "relation") || element.id == 0 {
                return None;
            }
            let tag = |key: &str| element.tags.get(key).map_or("", String::as_str);
            let first = |keys: &[&str]| {
                keys.iter()
                    .map(|key| tag(key))
                    .find(|s| !s.trim().is_empty())
                    .unwrap_or("")
            };
            let name = tag("name");
            let category = first(&["shop", "craft", "amenity", "office", "tourism", "leisure"]);
            if name.trim().is_empty() || category.is_empty() {
                return None;
            }
            let street = [tag("addr:housenumber"), tag("addr:street")]
                .into_iter()
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let formatted = [street.as_str(), tag("addr:city"), tag("addr:postcode")]
                .into_iter()
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            let website = [
                "website",
                "contact:website",
                "url",
                "facebook",
                "contact:facebook",
            ]
            .into_iter()
            .map(tag)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(";");
            let lead = Lead {
                name: name.to_owned(),
                category: category.replace('_', " "),
                phone: first(&["phone", "contact:phone"]).to_owned(),
                address: if tag("addr:full").is_empty() {
                    formatted
                } else {
                    tag("addr:full").to_owned()
                },
                hours: tag("opening_hours").to_owned(),
                website,
                services: String::new(),
            };
            let source = Source::Osm {
                reference: format!("{}/{}", element.kind, element.id),
            };
            Some(lead.checked().map(|lead| candidate(lead, source)))
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Format {
    Csv,
    Osm,
}

#[derive(Deserialize)]
pub(crate) struct Import {
    format: Format,
    content: String,
}

pub(crate) async fn demo() -> Result<Json<Vec<Candidate>>, ApiError> {
    Ok(Json(csv_leads(DEMO, Source::Fixture)?))
}

pub(crate) async fn import(Json(input): Json<Import>) -> Result<Json<Vec<Candidate>>, ApiError> {
    let mut leads = match input.format {
        Format::Csv => csv_leads(&input.content, Source::Operator)?,
        Format::Osm => osm_leads(&input.content)?,
    };
    if leads.is_empty() {
        return Err(invalid("No named businesses with a category were found."));
    }
    leads.sort_by_key(|lead| std::cmp::Reverse(lead.score));
    Ok(Json(leads))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorizes_domains_without_suffix_spoofing() {
        assert_eq!(
            website_signal("m.facebook.com/a"),
            WebsiteSignal::SocialOrDirectory
        );
        assert_eq!(
            website_signal("facebook.com.example.org"),
            WebsiteSignal::WebsiteListed
        );
        assert_eq!(
            website_signal("facebook.com/a; example.org"),
            WebsiteSignal::WebsiteListed
        );
        assert_eq!(website_signal("not a website"), WebsiteSignal::ReviewNeeded);
        assert_eq!(website_signal(""), WebsiteSignal::NoWebsiteListed);
    }

    #[test]
    fn csv_import_supports_quoted_fields_and_rejects_missing_required_values() {
        let rows = csv_leads(
            "name,category,address\n\"Shop, LLC\",Repair,\"1 Main, Sample City\"\n",
            Source::Operator,
        )
        .unwrap();
        assert_eq!(rows[0].lead.name, "Shop, LLC");
        assert_eq!(rows[0].lead.address, "1 Main, Sample City");
        assert!(csv_leads("name,category\n ,Repair", Source::Operator).is_err());
    }

    #[test]
    fn osm_import_keeps_the_listing_reference_and_considers_all_website_tags() {
        let rows = osm_leads(r#"{"elements":[{"type":"way","id":7,"tags":{"name":"Shop","shop":"car_repair","website":"facebook.com/shop","contact:website":"example.org"}}]}"#).unwrap();
        assert!(matches!(&rows[0].source, Source::Osm { reference } if reference == "way/7"));
        assert_eq!(rows[0].lead.category, "car repair");
        assert_eq!(rows[0].signal, WebsiteSignal::WebsiteListed);
    }
}
