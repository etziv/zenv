-- Sites may follow "whichever web server is running": service_id becomes
-- optional (NULL = served by every managed web server). Rebuilt because
-- SQLite cannot drop a NOT NULL constraint in place.
CREATE TABLE sites_v4 (
    id                      TEXT PRIMARY KEY,
    name                    TEXT NOT NULL,
    domain                  TEXT NOT NULL,
    project_root            TEXT NOT NULL,
    document_root           TEXT NOT NULL,
    project_type            TEXT NOT NULL,
    service_id              TEXT REFERENCES services (id) ON DELETE RESTRICT,
    strategy                TEXT NOT NULL CHECK (strategy IN ('static', 'reverse_proxy', 'php')),
    upstream_port           INTEGER CHECK (upstream_port BETWEEN 1 AND 65535),
    runtime_installation_id TEXT REFERENCES installations (id) ON DELETE RESTRICT,
    php_engine              TEXT NOT NULL DEFAULT 'auto' CHECK (php_engine IN ('auto', 'fastcgi', 'frankenphp')),
    https_enabled           INTEGER NOT NULL DEFAULT 0 CHECK (https_enabled IN (0, 1)),
    enabled                 INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    config_status           TEXT NOT NULL DEFAULT 'pending'
                            CHECK (config_status IN ('pending', 'applied', 'failed', 'disabled')),
    config_message          TEXT,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    CHECK (strategy <> 'reverse_proxy' OR upstream_port IS NOT NULL),
    CHECK (strategy <> 'php' OR runtime_installation_id IS NOT NULL)
);

INSERT INTO sites_v4 (id, name, domain, project_root, document_root, project_type, service_id, strategy,
                      upstream_port, runtime_installation_id, php_engine, https_enabled, enabled,
                      config_status, config_message, created_at, updated_at)
SELECT id, name, domain, project_root, document_root, project_type, service_id, strategy,
       upstream_port, runtime_installation_id, php_engine, https_enabled, enabled,
       config_status, config_message, created_at, updated_at
FROM sites;

DROP TABLE sites;
ALTER TABLE sites_v4 RENAME TO sites;
CREATE UNIQUE INDEX idx_sites_domain ON sites (domain COLLATE NOCASE);
CREATE INDEX idx_sites_service ON sites (service_id, enabled);
CREATE INDEX idx_sites_name ON sites (name COLLATE NOCASE);
CREATE INDEX idx_sites_runtime ON sites (runtime_installation_id);
