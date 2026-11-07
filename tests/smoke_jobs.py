"""Run the operator workflow before and after a Postgres-backed app restart."""

import json
import os
import sys
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen

FIRST = "10000000-0000-4000-8000-000000000001"
INTERRUPTED = "10000000-0000-4000-8000-000000000002"
EXHAUSTED = "10000000-0000-4000-8000-000000000003"


def api(path, body=None, status=200, authorized=True):
    headers = {"Content-Type": "application/json"}
    if authorized:
        headers["Authorization"] = "Bearer " + os.environ["OPERATOR_TOKEN"]
    data = None if body is None else json.dumps(body).encode()
    request = Request(f"{sys.argv[1]}/api/v1/{path}", data, headers)
    try:
        response = urlopen(request, timeout=10)
    except HTTPError as error:
        response = error
    with response:
        assert response.status == status, (path, response.status, response.read())
        assert response.headers.get("Cache-Control") == "no-store" or status >= 400
        return json.load(response)


def wait_for(job_id, stage):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        job = api(f"jobs/{job_id}")
        if job["stage"] == stage:
            return job
        time.sleep(0.25)
    raise AssertionError(f"Expected {stage}, got {job['stage']}")


assert api("jobs/config", authorized=False) == {"enabled": True}
api("jobs", status=401, authorized=False)
api(f"jobs/{FIRST}", status=401, authorized=False)
if sys.argv[2] == "before":
    candidate = {
        "lead": {"name": "Restart Test Garage", "category": "Auto repair",
                 "services": "Brake inspections"},
        "source": {"kind": "operator"},
        "confirmed": False,
    }
    api("jobs", {"id": FIRST, "candidate": candidate}, status=422)
    candidate["confirmed"] = True
    for _ in range(2):
        api("jobs", {"id": FIRST, "candidate": candidate}, status=202)
    changed = {**candidate, "lead": {**candidate["lead"], "name": "Different lead"}}
    api("jobs", {"id": FIRST, "candidate": changed}, status=409)
    job = wait_for(FIRST, "brief_review")
    assert job["variants"] is None
    assert [event["event"] for event in job["events"]] == ["queued", "claimed", "stage_changed"]
    assert len(api("jobs")) == 1
elif sys.argv[2] == "after":
    job = api(f"jobs/{FIRST}")
    assert job["stage"] == "brief_review" and job["variants"] is None
    assert job["brief"]["lead"]["services"] == "Brake inspections"
    for _ in range(2):
        api(f"jobs/{FIRST}/approve", {}, status=202)
    job = wait_for(FIRST, "preview_ready")
    assert [variant["id"] for variant in job["variants"]] == ["A", "B"]
    assert all("Brake inspections" in variant["html"] for variant in job["variants"])
    assert [event["stage"] for event in job["events"] if event["event"] == "stage_changed"] == [
        "brief_review", "building", "preview_ready",
    ]

    job = wait_for(INTERRUPTED, "brief_review")
    assert any(event["event"] == "reclaimed" and event["attempt"] == 2 for event in job["events"])
    job = wait_for(EXHAUSTED, "needs_operator")
    assert job["failure"] and job["brief"] is None
    api(f"jobs/{EXHAUSTED}/approve", {}, status=409)
else:
    raise AssertionError(f"Unknown test mode {sys.argv[2]}")

print(f"Saved jobs integration checks passed ({sys.argv[2]} restart).")
