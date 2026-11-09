-- Apps served from a Docker container: the container (and its compose
-- service, when it has one) Zenv starts and stops with the app. The app
-- itself is a reverse proxy to the container's published port.
CREATE TABLE docker_apps (
    site_id         TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE,
    container       TEXT NOT NULL,
    compose_project TEXT,
    compose_service TEXT,
    compose_dir     TEXT,
    -- JSON array of compose files
    compose_files   TEXT,
    autostart       INTEGER NOT NULL DEFAULT 1,
    updated_at      TEXT NOT NULL
);
