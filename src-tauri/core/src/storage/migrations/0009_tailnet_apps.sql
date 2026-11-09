-- Apps opened on the user's Tailscale network: `tailscale serve` passes
-- http://<this computer>:<port> to the app's web server on 127.0.0.1:<port>.
CREATE TABLE tailnet_apps (
    site_id    TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE,
    port       INTEGER NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);
