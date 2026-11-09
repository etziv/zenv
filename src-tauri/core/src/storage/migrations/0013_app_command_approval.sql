-- A command detected from a project (npm run dev, cargo run, ...) no longer
-- runs by itself: the user approves it first, by starting the app or saving
-- the command. Apps added before this change already ran theirs, so they
-- keep doing so: Zenv saves their detected command on its next start.
ALTER TABLE app_commands ADD COLUMN adopt_detected INTEGER NOT NULL DEFAULT 0;
INSERT INTO app_commands (site_id, command, autostart, updated_at, adopt_detected)
SELECT id, NULL, 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), 1
FROM sites
WHERE strategy = 'reverse_proxy'
  AND id NOT IN (SELECT site_id FROM app_commands)
  AND id NOT IN (SELECT site_id FROM docker_apps);
UPDATE app_commands SET adopt_detected = 1 WHERE command IS NULL AND adopt_detected = 0;
