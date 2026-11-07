use axum::Json;
use serde::{Deserialize, Serialize};

use crate::leads::{ApiError, Lead, Source, invalid};

#[derive(Deserialize, Serialize)]
struct Fact {
    field: String,
    value: String,
    confidence: f32,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Brief {
    lead: Lead,
    source: Source,
    facts: Vec<Fact>,
    unknowns: Vec<String>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct BriefRequest {
    lead: Lead,
    source: Source,
    confirmed: bool,
}

#[derive(Deserialize)]
pub(crate) struct PreviewRequest {
    #[serde(flatten)]
    candidate: BriefRequest,
    approved: bool,
}

#[derive(Serialize)]
pub(crate) struct Variant {
    id: &'static str,
    direction: &'static str,
    html: String,
}

impl BriefRequest {
    pub(crate) fn checked(mut self) -> Result<Self, ApiError> {
        if !self.confirmed {
            return Err(invalid("Confirm the candidate before preparing a brief."));
        }
        self.lead = self.lead.checked()?;
        if let Source::Osm { reference } = &self.source {
            let valid = reference.split_once('/').is_some_and(|(kind, id)| {
                matches!(kind, "node" | "way" | "relation")
                    && id.parse::<u64>().is_ok_and(|id| id > 0)
            });
            if !valid {
                return Err(invalid("The OpenStreetMap reference is invalid."));
            }
        }
        Ok(self)
    }
}

pub(crate) fn brief(request: BriefRequest) -> Result<Brief, ApiError> {
    let BriefRequest { lead, source, .. } = request.checked()?;
    let confidence = match source {
        Source::Fixture | Source::Operator => 1.0,
        Source::Osm { .. } => 0.8,
    };
    let mut facts = Vec::new();
    let mut unknowns = Vec::new();
    for (field, value) in [
        ("Name", &lead.name),
        ("Category", &lead.category),
        ("Phone", &lead.phone),
        ("Address", &lead.address),
        ("Hours", &lead.hours),
        ("Services", &lead.services),
    ] {
        if value.is_empty() {
            unknowns.push(field.to_owned());
        } else {
            facts.push(Fact {
                field: field.to_owned(),
                value: value.clone(),
                confidence,
            });
        }
    }
    Ok(Brief {
        lead,
        source,
        facts,
        unknowns,
    })
}

pub(crate) async fn prepare(Json(request): Json<BriefRequest>) -> Result<Json<Brief>, ApiError> {
    Ok(Json(brief(request)?))
}

pub(crate) async fn generate(
    Json(request): Json<PreviewRequest>,
) -> Result<Json<[Variant; 2]>, ApiError> {
    if !request.approved {
        return Err(invalid(
            "Approve the sourced brief before generating previews.",
        ));
    }
    let brief = brief(request.candidate)?;
    Ok(Json(variants(&brief)))
}

pub(crate) fn variants(brief: &Brief) -> [Variant; 2] {
    [
        variant(brief, "A", "Clean and modern"),
        variant(brief, "B", "Bold and local"),
    ]
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn variant(brief: &Brief, id: &'static str, direction: &'static str) -> Variant {
    let lead = &brief.lead;
    let name = escape(&lead.name);
    let category = escape(&lead.category);
    let phone = escape(&lead.phone);
    let digits: String = lead.phone.chars().filter(char::is_ascii_digit).collect();
    let contact = if (7..=15).contains(&digits.len())
        && lead
            .phone
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '+' | '(' | ')' | '-' | '.' | ' '))
    {
        let prefix = if lead.phone.starts_with('+') { "+" } else { "" };
        format!(r#"<a class="call" href="tel:{prefix}{digits}">Call {phone}</a>"#)
    } else {
        String::new()
    };
    let details = brief
        .facts
        .iter()
        .filter(|fact| {
            matches!(fact.field.as_str(), "Services" | "Address" | "Hours")
                || (fact.field == "Phone" && contact.is_empty())
        })
        .map(|fact| {
            format!(
                "<section><h2>{}</h2><p>{}</p></section>",
                fact.field,
                escape(&fact.value),
            )
        })
        .collect::<String>();
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<meta name="robots" content="noindex, nofollow">
<meta name="description" content="{category} concept preview for {name}">
<title>{name} · Concept preview</title>
<style>
* {{ box-sizing: border-box; }}
body {{ --background: #f3f7f4; --foreground: #172b23; --accent: #285a44; --panel: #fff;
  margin: 0; background: var(--background); color: var(--foreground);
  font: 1rem/1.6 system-ui, sans-serif; overflow-wrap: anywhere; }}
body[data-variant="B"] {{ --background: #242122; --foreground: #fff7e9; --accent: #ffc267; --panel: #332d2c; }}
header {{ padding: .7rem 1rem; text-align: center; border-bottom: 1px solid currentColor; font-size: .8rem; }}
main {{ width: min(100% - 2rem, 56rem); margin: 0 auto; padding: clamp(2rem, 8vw, 5rem) 0; }}
h1 {{ margin: .2rem 0 1.2rem; max-width: 15ch; font-size: clamp(2rem, 7vw, 5rem); line-height: 1.04; letter-spacing: -.045em; }}
.label {{ margin: 0; color: var(--accent); font-weight: 700; }}
.call {{ display: inline-block; color: var(--background); background: var(--accent); padding: .55rem 1rem; border-radius: .4rem; font-weight: 700; }}
.details {{ margin-top: 2rem; display: grid; gap: .8rem; grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr)); }}
section {{ background: var(--panel); padding: 1rem; border-radius: .5rem; }}
h2 {{ margin: 0 0 .25rem; font-size: .9rem; }}
section p {{ margin: 0; white-space: pre-wrap; }}
[data-variant="A"] .hero {{ text-align: center; }}
[data-variant="A"] h1 {{ margin-inline: auto; }}
[data-variant="B"] .hero {{ border-left: .35rem solid var(--accent); padding-left: 1rem; }}
[data-variant="B"] h1 {{ font-family: Georgia, serif; }}
@media (min-width: 42rem) {{
  [data-variant="B"] main {{ display: grid; grid-template-columns: 3fr 2fr; gap: 1.5rem; align-items: start; }}
  [data-variant="B"] .details {{ margin-top: 0; grid-template-columns: 1fr; }}
}}
</style>
</head>
<body data-variant="{id}">
<header>Concept preview prepared by your agency</header>
<main><div class="hero"><p class="label">{category}</p><h1>{name}</h1>{contact}</div><div class="details">{details}</div></main>
</body>
</html>"#
    );
    Variant {
        id,
        direction,
        html,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> BriefRequest {
        BriefRequest {
            lead: Lead {
                name: r#"Test "Co" <img src=x onerror=alert(1)>"#.to_owned(),
                category: "Car repair".to_owned(),
                phone: "+1 (202) 555-0147".to_owned(),
                services: "Brake inspections & oil changes".to_owned(),
                address: String::new(),
                hours: String::new(),
                website: String::new(),
            },
            source: Source::Fixture,
            confirmed: true,
        }
    }

    #[test]
    fn brief_requires_confirmation() {
        let mut request = request();
        request.confirmed = false;
        assert!(brief(request).is_err());
    }

    #[tokio::test]
    async fn variants_use_the_same_escaped_facts_and_require_approval() {
        let unapproved = PreviewRequest {
            candidate: request(),
            approved: false,
        };
        assert!(generate(Json(unapproved)).await.is_err());
        let approved = PreviewRequest {
            candidate: request(),
            approved: true,
        };
        let Json([a, b]) = generate(Json(approved)).await.unwrap();
        for variant in [&a, &b] {
            assert!(variant.html.contains("Brake inspections &amp; oil changes"));
            assert!(variant.html.contains("href=\"tel:+12025550147\""));
            assert!(
                variant
                    .html
                    .contains("Concept preview prepared by your agency")
            );
            assert!(variant.html.contains("&lt;img"));
            assert!(!variant.html.contains("<img"));
            assert!(!variant.html.contains("<h2>Hours</h2>"));
        }
        assert_ne!(a.html, b.html);
    }
}
