use std::{env, time::Duration};

use serde::Serialize;

const ENDPOINT: &str = "https://api.github.com/repos/Wuhpondiscord/wuhpwebbuild/actions/workflows/space-runtime.yml/dispatches";

#[derive(Serialize)]
struct Dispatch<'a> {
    #[serde(rename = "ref")]
    branch: &'static str,
    inputs: Event<'a>,
}

#[derive(Serialize)]
struct Event<'a> {
    revision: &'a str,
    event: &'a str,
}

pub async fn report(revision: &str, event: &str) {
    if env::var("SPACE_ID").as_deref() != Ok("wuhp/wuhpwebbuild")
        || revision.len() != 40
        || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return;
    }
    let Ok(token) = env::var("Gh_Token") else {
        tracing::warn!("Gh_Token is missing; runtime event could not be sent");
        return;
    };
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            tracing::warn!("could not initialize the runtime event client");
            return;
        }
    };
    let payload = Dispatch {
        branch: "main",
        inputs: Event { revision, event },
    };

    match client
        .post(ENDPOINT)
        .header("User-Agent", "wuhpwebbuild-space")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            tracing::info!(revision, event, "runtime event accepted by GitHub");
        }
        Ok(response) => tracing::warn!(
            status = response.status().as_u16(),
            "GitHub rejected the runtime event"
        ),
        Err(_) => tracing::warn!("could not reach GitHub to send the runtime event"),
    }
}
