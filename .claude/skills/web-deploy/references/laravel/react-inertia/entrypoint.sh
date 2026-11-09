#!/bin/sh
# entrypoint.sh - boot container Laravel. Generic, dipakai semua stack.
set -e

cd /app

# Storage skeleton - named volume mount menutupi isi bawaan image, jadi
# struktur folder di-buat ulang tiap boot.
mkdir -p \
    storage/app/public \
    storage/framework/cache/data \
    storage/framework/sessions \
    storage/framework/views \
    storage/framework/testing \
    storage/logs \
    bootstrap/cache
chown -R www-data:www-data storage bootstrap/cache 2>/dev/null || true
chmod -R ug+rw storage bootstrap/cache 2>/dev/null || true

# Compose tidak memakai env_file (lihat docker-compose.yml): ambil DB_HOST/DB_PORT
# dari /app/.env kalau tidak ada di environment.
envget() { [ -f .env ] && grep -E "^$1=" .env | tail -1 | cut -d= -f2- | tr -d '\r"' ; }
DB_HOST="${DB_HOST:-$(envget DB_HOST)}"
DB_PORT="${DB_PORT:-$(envget DB_PORT)}"

# Tunggu database kalau DB_HOST diset.
if [ -n "$DB_HOST" ] && [ -n "$DB_PORT" ]; then
    echo "Waiting for ${DB_HOST}:${DB_PORT}..."
    timeout=60
    while ! nc -z "$DB_HOST" "$DB_PORT" >/dev/null 2>&1; do
        timeout=$((timeout - 1))
        [ "$timeout" -le 0 ] && { echo "DB timeout, lanjut saja."; break; }
        sleep 1
    done
fi

# Generate APP_KEY kalau belum ada (idempotent).
if [ -z "$APP_KEY" ] && ! grep -q "^APP_KEY=base64:" .env 2>/dev/null; then
    php artisan key:generate --force --no-interaction || true
fi

php artisan storage:link --no-interaction || true

# Migrate + seed di service utama saja (RUN_MIGRATIONS=true). Sidecar
# (scheduler/queue/reverb) di-set false. Pastikan SEMUA seeder idempotent
# (firstOrCreate / updateOrCreate), bukan create().
if [ "${RUN_MIGRATIONS:-true}" = "true" ]; then
    php artisan migrate --force --no-interaction || true
    php artisan db:seed --force --no-interaction || true
fi

# Cache config/route/view/event untuk performa.
php artisan config:cache --no-interaction || true
php artisan route:cache --no-interaction || true
php artisan view:cache --no-interaction || true
php artisan event:cache --no-interaction || true

exec "$@"
