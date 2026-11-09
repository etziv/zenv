-- Registered runtime / server installations. Zenv never deletes the
-- directories these rows point to.
CREATE TABLE installations (
    id              TEXT PRIMARY KEY,
    component       TEXT NOT NULL,
    version         TEXT NOT NULL,
    version_detail  TEXT NOT NULL,
    root_path       TEXT NOT NULL,
    executable_path TEXT NOT NULL,
    source          TEXT NOT NULL CHECK (source IN ('external', 'managed')),
    is_default      INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    detected_at     TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_installations_executable
    ON installations (component, executable_path COLLATE NOCASE);
CREATE UNIQUE INDEX idx_installations_one_default
    ON installations (component) WHERE is_default = 1;

-- Managed service instances (one server process each).
CREATE TABLE services (
    id              TEXT PRIMARY KEY,
    component       TEXT NOT NULL,
    name            TEXT NOT NULL,
    installation_id TEXT NOT NULL REFERENCES installations (id) ON DELETE RESTRICT,
    listen_address  TEXT NOT NULL,
    http_port       INTEGER NOT NULL CHECK (http_port BETWEEN 1 AND 65535),
    autostart       INTEGER NOT NULL DEFAULT 0 CHECK (autostart IN (0, 1)),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_services_name ON services (name COLLATE NOCASE);

-- Ledger of files written by Zenv; anything absent here is never overwritten.
CREATE TABLE managed_files (
    path       TEXT PRIMARY KEY COLLATE NOCASE,
    owner_kind TEXT NOT NULL,
    owner_id   TEXT NOT NULL,
    sha256     TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE service_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    service_id TEXT NOT NULL REFERENCES services (id) ON DELETE CASCADE,
    level      TEXT NOT NULL CHECK (level IN ('info', 'warning', 'error')),
    message    TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_service_events_service ON service_events (service_id, id DESC);
