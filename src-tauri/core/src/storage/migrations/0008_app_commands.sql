-- How Zenv starts the app behind a proxy app (cargo run, npm run dev, ...).
-- No row, or a NULL command, means "use the command detected from the project".
CREATE TABLE app_commands (
    site_id    TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE,
    command    TEXT,
    autostart  INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL
);
