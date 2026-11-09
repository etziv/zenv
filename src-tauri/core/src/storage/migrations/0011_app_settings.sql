-- Small Zenv settings (e.g. OmniRoute auto-update on/off).
CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
