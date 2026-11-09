# syntax=docker/dockerfile:1

# Stage: frontend - build Vite assets.
# VITE_* args wajib di-pass saat build karena di-bake ke JS bundle final.
FROM node:26-alpine AS frontend
ARG VITE_APP_NAME=MyApp
ARG VITE_REVERB_APP_KEY
ARG VITE_REVERB_HOST=localhost
ARG VITE_REVERB_PORT=8090
ARG VITE_REVERB_SCHEME=http
ENV VITE_APP_NAME=$VITE_APP_NAME \
    VITE_REVERB_APP_KEY=$VITE_REVERB_APP_KEY \
    VITE_REVERB_HOST=$VITE_REVERB_HOST \
    VITE_REVERB_PORT=$VITE_REVERB_PORT \
    VITE_REVERB_SCHEME=$VITE_REVERB_SCHEME
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci
COPY resources ./resources
COPY public ./public
COPY vite.config.js tsconfig.json components.json ./
COPY .prettierrc* eslint.config.js* ./
RUN npm run build

# Stage: vendor - download composer deps.
FROM composer:2 AS vendor
WORKDIR /app
COPY composer.json composer.lock ./
RUN composer install --no-dev --no-scripts --no-autoloader \
    --prefer-dist --no-interaction --no-progress --ignore-platform-reqs

# Stage: app - FrankenPHP + Octane runtime, image final.
FROM dunglas/frankenphp:1-php8.4-alpine AS app

RUN install-php-extensions pcntl pdo_mysql bcmath intl zip gd redis opcache

COPY --from=composer:2 /usr/bin/composer /usr/bin/composer

WORKDIR /app
COPY --from=vendor /app/vendor ./vendor
COPY . .
COPY --from=frontend /app/public/build ./public/build

# Autoload optimized TAPI TIDAK authoritative: dengan bind-mount, class PHP baru
# di source harus ke-load via PSR-4 fallback tanpa dump-autoload ulang.
# (Balikkan ke --classmap-authoritative kalau pakai pola COPY-source-in-image.)
RUN composer dump-autoload --optimize --no-dev

RUN mkdir -p storage/framework/{cache,sessions,views,testing} storage/logs bootstrap/cache \
    && chown -R www-data:www-data storage bootstrap/cache \
    && chmod -R ug+rw storage bootstrap/cache

# OPcache tuning (validate_timestamps=1 untuk bind-mount + Octane, lihat file).
COPY docker/php/opcache.ini /usr/local/etc/php/conf.d/opcache.ini

EXPOSE 8080

COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENTRYPOINT ["entrypoint.sh"]
CMD ["php", "artisan", "octane:start", "--server=frankenphp", "--host=0.0.0.0", "--port=8080"]
