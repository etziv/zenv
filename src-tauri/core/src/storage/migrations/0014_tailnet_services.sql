-- An app on the tailnet may have its own name as a Tailscale Service
-- (https://<service>.<tailnet>.ts.net) instead of this computer's name and a port.
ALTER TABLE tailnet_apps ADD COLUMN service TEXT;
CREATE UNIQUE INDEX tailnet_apps_service ON tailnet_apps(service) WHERE service IS NOT NULL;
