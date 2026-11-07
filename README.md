---
title: Wuhp Web Build
emoji: 🛠️
colorFrom: green
colorTo: purple
sdk: docker
app_port: 7860
pinned: false
---

# Wuhp Web Build

The Phase 1 Lead-to-Launch workspace runs on a Rust server with a React interface
and a health endpoint at `/health` on port 7860.

## Try the workflow

1. Load fictional sample leads or import CSV or an OpenStreetMap Overpass JSON export.
2. Screen the website tags, review the listing independently, and confirm a candidate.
3. Approve the sourced business brief and compare Site A and Site B.
4. Download either standalone HTML concept. Previews show a concept banner, omit
   missing facts, and are sandboxed in the workspace.

CSV requires `name` and `category`. Optional columns are `phone`, `address`,
`hours`, `website`, and `services`. Imports accept up to 200 rows or OSM elements.
Session demo work clears when you reload.

## Saved jobs

With Postgres and an operator token configured, **Saved jobs** opens a single
operator workspace. Confirm a candidate, unlock the workspace, and queue the
lead. The worker saves a sourced brief and waits for approval before it builds
and saves both previews. Reopen recent jobs after a restart and expand **Job
history** to see the recorded stages and attempts.

Use a randomly generated `OPERATOR_TOKEN` of 32–256 printable ASCII characters
with no whitespace. Keep it private and set it in the app's environment. In
Docker Compose, set the variable in the terminal environment or an ignored
`.env` file before running `docker compose up --build`. On Hugging Face set both
`OPERATOR_TOKEN` and a Postgres `DATABASE_URL` reachable from the Space as secrets.
The public session demo remains available. The workspace currently uses a
shared operator token and fixed templates. Live discovery, Gemini agents,
automated reviews, generated-site hosting, and analytics are later slices.

## Local development

Run `docker compose up --build` and open [the app](http://localhost:7860).
Compose starts Postgres and the app applies the migrations. `/health` returns
the running revision and database status. The Space can start in session demo mode
without Postgres. Set `DATABASE_URL` as a Space secret to enable Postgres and run
the migrations on startup.

## Deployment

Push to GitHub `main`. The **Test and deploy** workflow builds the Docker image,
checks the React bundle and health endpoint, and tests the pipeline, approval
gate, and interrupted-job recovery against Postgres. After the test passes, it uses the GitHub Actions secret
`HF_TOKEN` to upload the tested revision to `wuhp/wuhpwebbuild`. The workflow
waits for the Space to report that revision as healthy. The token needs write
access to that Space.

The Space reads the exact secret name `Gh_Token` at runtime. Use a GitHub
fine-grained token scoped to `Wuhpondiscord/wuhpwebbuild` with **Actions: Read and
write**. The app sends the running GitHub revision and either
`server_started` or `database_initialization_failed` to the **Hugging Face
runtime events** workflow. Each event appears in the run summary and in the
`space-runtime` JSON artifact for 30 days. Build logs and raw container output
are available in the Space's Logs tab. Runtime events contain only allowlisted
fields because the GitHub repository and its Actions runs are public.
