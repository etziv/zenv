-- The version of each language or tool Zenv put on the user's PATH, and the
-- folders it added (so they can be replaced or removed again exactly).
CREATE TABLE path_choices (
    component       TEXT PRIMARY KEY,
    installation_id TEXT NOT NULL,
    dirs            TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
