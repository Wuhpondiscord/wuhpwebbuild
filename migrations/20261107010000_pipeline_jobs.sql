INSERT INTO workspaces (id, name)
VALUES ('00000000-0000-0000-0000-000000000001', 'Operator workspace')
ON CONFLICT (id) DO NOTHING;

CREATE TABLE jobs (
    workspace_id UUID NOT NULL REFERENCES workspaces (id),
    id UUID NOT NULL,
    stage TEXT NOT NULL DEFAULT 'researching'
        CHECK (stage IN ('researching', 'brief_review', 'building', 'preview_ready', 'needs_operator')),
    candidate JSONB NOT NULL,
    brief JSONB,
    variants JSONB,
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt BETWEEN 0 AND 3),
    lease_until TIMESTAMPTZ,
    failure TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, id)
);

CREATE INDEX jobs_pending_idx ON jobs (workspace_id, created_at)
    WHERE stage IN ('researching', 'building');

CREATE TABLE job_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    workspace_id UUID NOT NULL,
    job_id UUID NOT NULL,
    stage TEXT NOT NULL,
    event TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (workspace_id, job_id) REFERENCES jobs (workspace_id, id)
);

CREATE INDEX job_events_job_idx ON job_events (workspace_id, job_id, id);

CREATE FUNCTION record_job_event() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE event_name TEXT;
BEGIN
    IF TG_OP = 'INSERT' THEN
        event_name := 'queued';
    ELSIF NEW.stage <> OLD.stage THEN
        event_name := 'stage_changed';
    ELSIF NEW.attempt > OLD.attempt THEN
        event_name := CASE WHEN OLD.attempt = 0 THEN 'claimed' ELSE 'reclaimed' END;
    ELSE
        RETURN NEW;
    END IF;

    INSERT INTO job_events (workspace_id, job_id, stage, event, attempt)
    VALUES (NEW.workspace_id, NEW.id, NEW.stage, event_name, NEW.attempt);
    RETURN NEW;
END;
$$;

CREATE TRIGGER job_event AFTER INSERT OR UPDATE ON jobs
FOR EACH ROW EXECUTE FUNCTION record_job_event();
