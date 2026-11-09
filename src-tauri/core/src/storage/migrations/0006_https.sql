-- HTTPS: web servers get a TLS port (Zenv listens on it only while a site
-- served by that server has HTTPS on), and sites may redirect HTTP to HTTPS.
ALTER TABLE services ADD COLUMN https_port INTEGER CHECK (https_port BETWEEN 1 AND 65535);
UPDATE services SET https_port = 443 WHERE component IN ('apache', 'nginx');
ALTER TABLE sites ADD COLUMN https_redirect INTEGER NOT NULL DEFAULT 0 CHECK (https_redirect IN (0, 1));
