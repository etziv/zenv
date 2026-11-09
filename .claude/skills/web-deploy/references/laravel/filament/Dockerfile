# syntax=docker/dockerfile:1
# Dockerfile - Laravel + Filament. Plain FrankenPHP (per-request fresh boot).
#
# Catatan: SENGAJA tidak pakai Octane worker mode.
# Filament v4/v5 + Livewire 3/4 + Shield punya state leak issues di Octane (blank
# pages, permission cache poisoning, panel registry static state). Plain
# frankenphp php-server lebih reliable + tetap 2-3x lebih cepat dari PHP-FPM.

# Stage: frontend - build Vite assets (custom Filament theme / app CSS).
# HAPUS stage ini + COPY --from=frontend di bawah kalau project TIDAK punya
# custom theme/asset (tidak ada package.json / vite.config.js).
FROM node:26-alpine AS frontend
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci
COPY resources ./resources
COPY vite.config.js ./
RUN npm run build

# Stage: vendor - download composer deps. --ignore-platform-reqs karena
# ext-* yang dibutuhkan ada di runtime image (stage app), bukan di composer:2.
FROM composer:2 AS vendor
WORKDIR /app
COPY composer.json composer.lock ./
RUN composer install --no-dev --no-scripts --no-autoloader \
    --prefer-dist --no-interaction --no-progress --ignore-platform-reqs

# Stage: app - FrankenPHP runtime, image final.
# Samakan tag php8.x dengan versi PHP di composer.lock.
FROM dunglas/frankenphp:1-php8.4-alpine AS app

# Ekstensi standar Filament. Tambah pdo_pgsql kalau ada koneksi PostgreSQL legacy.
# exif sering dibutuhkan Filament untuk upload/preview gambar.
# redis pin ke versi spesifik supaya gak ke-bite kalau PECL latest issues.
RUN install-php-extensions \
    pcntl \
    pdo_mysql \
    bcmath \
    intl \
    zip \
    gd \
    redis-6.3.0 \
    opcache \
    exif

# Composer binary untuk dump-autoload (runtime image tidak punya composer).
COPY --from=composer:2 /usr/bin/composer /usr/bin/composer

WORKDIR /app
COPY --from=vendor /app/vendor ./vendor
COPY . .
# Hapus baris ini kalau stage frontend dihapus.
COPY --from=frontend /app/public/build ./public/build

# Autoload optimized TAPI TIDAK authoritative: dengan bind-mount, class PHP baru
# (Resource/Controller baru) di source harus ke-load via PSR-4 fallback tanpa
# dump-autoload ulang. classmap-authoritative menolak class di luar classmap ->
# tidak cocok live-sync. (Balikkan ke --classmap-authoritative kalau pakai pola
# COPY-source-in-image.)
RUN composer dump-autoload --optimize --no-dev

RUN mkdir -p storage/framework/{cache,sessions,views,testing} storage/logs bootstrap/cache \
    && chown -R www-data:www-data storage bootstrap/cache \
    && chmod -R ug+rw storage bootstrap/cache

# OPcache tuning untuk production. Buat docker/php/opcache.ini di repo
# (lihat catatan di README).
COPY docker/php/opcache.ini /usr/local/etc/php/conf.d/opcache.ini

EXPOSE 8080

COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENTRYPOINT ["entrypoint.sh"]
# Plain FrankenPHP php-server mode (BUKAN Octane worker).
# Setiap request fresh boot Laravel - cocok untuk Filament+Livewire yang
# stateful, hindari state leak antar request.
CMD ["frankenphp", "php-server", "--listen", ":8080", "--root", "/app/public"]
