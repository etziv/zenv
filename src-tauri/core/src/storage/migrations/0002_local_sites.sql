-- Local sites: one row per registered project/domain. Deleting a row never
-- touches the project directory.
CREATE TABLE sites (
    id                      TEXT PRIMARY KEY,
    name                    TEXT NOT NULL,
    domain                  TEXT NOT NULL,
    project_root            TEXT NOT NULL,
    document_root           TEXT NOT NULL,
    project_type            TEXT NOT NULL,
    service_id              TEXT NOT NULL REFERENCES services (id) ON DELETE RESTRICT,
    strategy                TEXT NOT NULL CHECK (strategy IN ('static', 'reverse_proxy')),
    upstream_port           INTEGER CHECK (upstream_port BETWEEN 1 AND 65535),
    runtime_installation_id TEXT REFERENCES installations (id) ON DELETE SET NULL,
    https_enabled           INTEGER NOT NULL DEFAULT 0 CHECK (https_enabled IN (0, 1)),
    enabled                 INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    config_status           TEXT NOT NULL DEFAULT 'pending'
                            CHECK (config_status IN ('pending', 'applied', 'failed', 'disabled')),
    config_message          TEXT,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    -- A reverse proxy without a target is never valid.
    CHECK (strategy <> 'reverse_proxy' OR upstream_port IS NOT NULL)
);
CREATE UNIQUE INDEX idx_sites_domain ON sites (domain COLLATE NOCASE);
CREATE INDEX idx_sites_service ON sites (service_id, enabled);
CREATE INDEX idx_sites_name ON sites (name COLLATE NOCASE);
