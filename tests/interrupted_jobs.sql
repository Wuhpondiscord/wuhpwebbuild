-- Simulate workers that stopped while holding a claim before the app restarts.
INSERT INTO jobs (workspace_id, id, candidate)
SELECT jobs.workspace_id, interrupted.id::uuid, jobs.candidate
FROM jobs
CROSS JOIN (VALUES
    ('10000000-0000-4000-8000-000000000002'),
    ('10000000-0000-4000-8000-000000000003')
) AS interrupted(id)
WHERE jobs.id = '10000000-0000-4000-8000-000000000001';

UPDATE jobs
SET attempt = CASE WHEN id = '10000000-0000-4000-8000-000000000002' THEN 1 ELSE 3 END,
    lease_until = clock_timestamp() - interval '1 second'
WHERE id IN ('10000000-0000-4000-8000-000000000002', '10000000-0000-4000-8000-000000000003');
