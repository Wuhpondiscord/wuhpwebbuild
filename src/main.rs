mod leads;
mod previews;
mod telemetry;

use std::{env, error::Error, io, time::Duration};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Serialize;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower_http::services::ServeDir;

const REVISION: &str = include_str!("../release.txt");

#[derive(Serialize)]
struct Health {
    status: &'static str,
    revision: &'static str,
    database: &'static str,
}

async fn health(State(database): State<Option<PgPool>>) -> (StatusCode, Json<Health>) {
    let (status, database) = match database {
        None => (StatusCode::OK, "not_configured"),
        Some(pool) => match tokio::time::timeout(
            Duration::from_secs(2),
            sqlx::query("SELECT 1").execute(&pool),
        )
        .await
        {
            Ok(Ok(_)) => (StatusCode::OK, "ready"),
            _ => (StatusCode::SERVICE_UNAVAILABLE, "unreachable"),
        },
    };
    let response = Health {
        status: if status.is_success() {
            "ok"
        } else {
            "degraded"
        },
        revision: REVISION.trim(),
        database,
    };
    (status, Json(response))
}

async fn initialize_database() -> Result<Option<PgPool>, Box<dyn Error>> {
    let url = match env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(env::VarError::NotPresent) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(Some(pool))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt().json().init();
    let revision = REVISION.trim();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:7860").await?;
    let database = match initialize_database().await {
        Ok(database) => database,
        Err(_) => {
            tracing::error!(revision, "database initialization failed");
            telemetry::report(revision, "database_initialization_failed").await;
            return Err(io::Error::other("database initialization failed"));
        }
    };
    let database_status = if database.is_some() {
        "ready"
    } else {
        "not_configured"
    };
    let web_dir = env::var("WEB_DIST").unwrap_or_else(|_| "web/dist".to_owned());
    let pipeline = Router::new()
        .route("/leads/demo", get(leads::demo))
        .route("/leads/import", post(leads::import))
        .route("/brief", post(previews::prepare))
        .route("/previews", post(previews::generate))
        .layer(DefaultBodyLimit::max(192 * 1024));
    let app = Router::new()
        .nest("/api/v1", pipeline)
        .route("/health", get(health))
        .fallback_service(ServeDir::new(web_dir))
        .with_state(database);

    tracing::info!(
        revision,
        database = database_status,
        "server started on port 7860"
    );
    tokio::spawn(telemetry::report(revision, "server_started"));
    axum::serve(listener, app).await
}
