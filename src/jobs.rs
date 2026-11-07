use std::{env, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use subtle::ConstantTimeEq;

use crate::{
    leads::{ApiError, error, invalid},
    previews::{self, Brief, BriefRequest},
};

const WORKSPACE: &str = "00000000-0000-0000-0000-000000000001";
const CLAIM_INTERVAL: Duration = Duration::from_secs(1);

type QueueState = Option<Arc<Queue>>;

pub(crate) struct Queue {
    pool: PgPool,
    token: String,
}

#[derive(Deserialize)]
struct CreateJob {
    id: String,
    candidate: BriefRequest,
}

#[derive(Serialize, sqlx::FromRow)]
struct JobSummary {
    id: String,
    name: String,
    stage: String,
}

#[derive(Serialize, sqlx::FromRow)]
struct Job {
    id: String,
    name: String,
    stage: String,
    brief: Option<Value>,
    variants: Option<Value>,
    failure: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct JobEvent {
    stage: String,
    event: String,
    attempt: i32,
}

#[derive(Serialize)]
struct JobDetail {
    #[serde(flatten)]
    job: Job,
    events: Vec<JobEvent>,
}

#[derive(sqlx::FromRow)]
struct Claim {
    id: String,
    stage: String,
    attempt: i32,
    candidate: Value,
    brief: Option<Value>,
}

impl Queue {
    pub(crate) fn start(database: Option<&PgPool>) -> QueueState {
        let database = database?;
        let token = env::var("OPERATOR_TOKEN").ok().filter(|token| {
            (32..=256).contains(&token.len()) && token.bytes().all(|byte| byte.is_ascii_graphic())
        })?;
        let queue = Arc::new(Self {
            pool: database.clone(),
            token,
        });
        tokio::spawn(Arc::clone(&queue).work());
        Some(queue)
    }

    async fn work(self: Arc<Self>) {
        loop {
            match self.process_next().await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(_) => tracing::warn!("job queue temporarily unavailable"),
            }
            tokio::time::sleep(CLAIM_INTERVAL).await;
        }
    }

    async fn process_next(&self) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "UPDATE jobs SET stage = 'needs_operator', lease_until = NULL,
                 failure = 'The worker lost its lease three times.'
             WHERE workspace_id = $1::uuid AND stage IN ('researching', 'building')
                 AND attempt = 3 AND lease_until <= clock_timestamp()",
        )
        .bind(WORKSPACE)
        .execute(&self.pool)
        .await?;

        let claim = sqlx::query_as::<_, Claim>(
            "WITH next AS (
                 SELECT id FROM jobs WHERE workspace_id = $1::uuid
                     AND stage IN ('researching', 'building') AND attempt < 3
                     AND (lease_until IS NULL OR lease_until <= clock_timestamp())
                 ORDER BY created_at, id LIMIT 1 FOR UPDATE SKIP LOCKED
             )
             UPDATE jobs SET attempt = attempt + 1, lease_until = clock_timestamp() + interval '30 seconds'
             FROM next WHERE jobs.workspace_id = $1::uuid AND jobs.id = next.id
             RETURNING jobs.id::text AS id, stage, attempt, candidate, brief",
        )
        .bind(WORKSPACE)
        .fetch_optional(&self.pool)
        .await?;

        let Some(claim) = claim else {
            return Ok(false);
        };
        let (stage, brief, variants, failure) = match process(&claim) {
            Ok(Step::Brief(brief)) => ("brief_review", Some(brief), None, None),
            Ok(Step::Previews(variants)) => ("preview_ready", None, Some(variants), None),
            Err(reason) => ("needs_operator", None, None, Some(reason)),
        };
        sqlx::query(
            "UPDATE jobs SET stage = $5, brief = COALESCE($6, brief), variants = $7,
                 failure = $8, attempt = 0, lease_until = NULL
             WHERE workspace_id = $1::uuid AND id = $2::uuid AND stage = $3 AND attempt = $4
                 AND lease_until > clock_timestamp()",
        )
        .bind(WORKSPACE)
        .bind(claim.id)
        .bind(claim.stage)
        .bind(claim.attempt)
        .bind(stage)
        .bind(brief)
        .bind(variants)
        .bind(failure)
        .execute(&self.pool)
        .await?;
        Ok(true)
    }
}

enum Step {
    Brief(Value),
    Previews(Value),
}

fn process(claim: &Claim) -> Result<Step, &'static str> {
    match claim.stage.as_str() {
        "researching" => {
            let request = serde_json::from_value(claim.candidate.clone())
                .map_err(|_| "The saved candidate is invalid.")?;
            let brief = previews::brief(request)
                .map_err(|_| "The saved candidate could not be verified.")?;
            serde_json::to_value(brief)
                .map(Step::Brief)
                .map_err(|_| "The brief could not be saved.")
        }
        "building" => {
            let brief: Brief = serde_json::from_value(claim.brief.clone().unwrap_or(Value::Null))
                .map_err(|_| "The approved brief could not be read.")?;
            serde_json::to_value(previews::variants(&brief))
                .map(Step::Previews)
                .map_err(|_| "The previews could not be saved.")
        }
        _ => Err("The job has an unknown stage."),
    }
}

fn authorized(queue: &QueueState, headers: &HeaderMap) -> Result<Arc<Queue>, ApiError> {
    let queue = queue.as_ref().ok_or_else(|| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Saved jobs are disabled on this deployment.",
        )
    })?;
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !bool::from(bearer.as_bytes().ct_eq(queue.token.as_bytes())) {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "Enter a valid operator token.",
        ));
    }
    Ok(Arc::clone(queue))
}

fn database_error(_: sqlx::Error) -> ApiError {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Saved jobs are temporarily unavailable.",
    )
}

fn job_id(id: &str) -> Result<&str, ApiError> {
    if id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
    {
        Ok(id)
    } else {
        Err(invalid("A job ID must be a UUID."))
    }
}

fn private_response(body: impl Serialize) -> impl IntoResponse {
    ([(header::CACHE_CONTROL, "no-store")], Json(body))
}

pub(crate) fn router(queue: QueueState) -> Router<Option<PgPool>> {
    Router::new()
        .route("/jobs/config", get(config))
        .route("/jobs", get(list).post(create))
        .route("/jobs/{id}", get(detail))
        .route("/jobs/{id}/approve", post(approve))
        .with_state(queue)
}

async fn config(State(queue): State<QueueState>) -> impl IntoResponse {
    private_response(serde_json::json!({ "enabled": queue.is_some() }))
}

async fn list(
    State(queue): State<QueueState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let queue = authorized(&queue, &headers)?;
    let jobs = sqlx::query_as::<_, JobSummary>(
        "SELECT id::text AS id, candidate #>> '{lead,name}' AS name, stage
         FROM jobs WHERE workspace_id = $1::uuid ORDER BY created_at DESC, id LIMIT 25",
    )
    .bind(WORKSPACE)
    .fetch_all(&queue.pool)
    .await
    .map_err(database_error)?;
    Ok(private_response(jobs))
}

async fn create(
    State(queue): State<QueueState>,
    headers: HeaderMap,
    Json(input): Json<CreateJob>,
) -> Result<impl IntoResponse, ApiError> {
    let queue = authorized(&queue, &headers)?;
    let id = job_id(&input.id)?;
    let candidate = input.candidate.checked()?;
    let candidate = serde_json::to_value(candidate).map_err(|_| invalid("Invalid candidate."))?;
    sqlx::query(
        "INSERT INTO jobs (workspace_id, id, candidate) VALUES ($1::uuid, $2::uuid, $3)
         ON CONFLICT (workspace_id, id) DO NOTHING",
    )
    .bind(WORKSPACE)
    .bind(id)
    .bind(&candidate)
    .execute(&queue.pool)
    .await
    .map_err(database_error)?;
    let same: bool = sqlx::query_scalar(
        "SELECT candidate = $3 FROM jobs WHERE workspace_id = $1::uuid AND id = $2::uuid",
    )
    .bind(WORKSPACE)
    .bind(id)
    .bind(candidate)
    .fetch_one(&queue.pool)
    .await
    .map_err(database_error)?;
    if !same {
        return Err(error(
            StatusCode::CONFLICT,
            "This job ID belongs to a different candidate.",
        ));
    }
    Ok((
        StatusCode::ACCEPTED,
        private_response(serde_json::json!({ "id": id })),
    ))
}

async fn detail(
    State(queue): State<QueueState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let queue = authorized(&queue, &headers)?;
    let id = job_id(&id)?;
    let mut tx = queue.pool.begin().await.map_err(database_error)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    let job = sqlx::query_as::<_, Job>(
        "SELECT id::text AS id, candidate #>> '{lead,name}' AS name, stage, brief, variants, failure
         FROM jobs WHERE workspace_id = $1::uuid AND id = $2::uuid",
    ).bind(WORKSPACE).bind(id).fetch_optional(&mut *tx).await.map_err(database_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Job not found."))?;
    let events = sqlx::query_as::<_, JobEvent>(
        "SELECT stage, event, attempt FROM job_events
         WHERE workspace_id = $1::uuid AND job_id = $2::uuid ORDER BY id",
    )
    .bind(WORKSPACE)
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(private_response(JobDetail { job, events }))
}

async fn approve(
    State(queue): State<QueueState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let queue = authorized(&queue, &headers)?;
    let id = job_id(&id)?;
    let advanced = sqlx::query(
        "UPDATE jobs SET stage = 'building'
         WHERE workspace_id = $1::uuid AND id = $2::uuid AND stage = 'brief_review'",
    )
    .bind(WORKSPACE)
    .bind(id)
    .execute(&queue.pool)
    .await
    .map_err(database_error)?;
    if advanced.rows_affected() == 0 {
        let stage: Option<String> = sqlx::query_scalar(
            "SELECT stage FROM jobs WHERE workspace_id = $1::uuid AND id = $2::uuid",
        )
        .bind(WORKSPACE)
        .bind(id)
        .fetch_optional(&queue.pool)
        .await
        .map_err(database_error)?;
        match stage.as_deref() {
            Some("building" | "preview_ready") => {}
            Some(_) => {
                return Err(error(
                    StatusCode::CONFLICT,
                    "The brief is not ready for approval.",
                ));
            }
            None => return Err(error(StatusCode::NOT_FOUND, "Job not found.")),
        }
    }
    Ok((
        StatusCode::ACCEPTED,
        private_response(serde_json::json!({ "id": id })),
    ))
}
