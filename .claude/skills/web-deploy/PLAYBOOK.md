# Laravel Docker Deployment Playbook

Panduan deploy aplikasi Laravel (Octane/FrankenPHP - Inertia/React maupun
Filament) ke server **tanpa git binary di host**. Dirangkum dari error nyata
saat setup produksi - ikuti checklist preventif di bawah supaya project lain
tidak mengulang error yang sama.

Lihat juga `SKILL.md` (cara pakai) dan template per-stack di folder
`references/laravel/react-inertia/` dan `references/laravel/filament/`.

---

## 1. Arsitektur

```
LAPTOP (dev)                      SERVER (production)
+----------------+                +------------------------------+
| code + git push|   github       | /root/<proj>/                |
| origin main    | -------------> |   deploy.sh    (entry point)  |
+----------------+                |   docker-compose.yml (sync)   |
                                  |   Dockerfile         (sync)   |
                                  |   entrypoint.sh      (sync)   |
                                  |                              |
                                  | /home/<proj>/src/            |
                                  |   .git/   (git repo asli)    |
                                  |   .env    (manual, secret)   |
                                  |   <source code lengkap>      |
                                  +------------------------------+
```

Prinsip:

- **Host server tidak perlu git** - sync source pakai container `alpine/git`.
- **Source code di host** (`/home/<proj>/src/`) - mudah di-inspect, ada `.git`
  untuk lihat history/commit date.
- **`/root/<proj>/`** = direktori operasional, mirror folder `docker/` dari repo,
  auto-sync tiap deploy.
- **`.env` di host**, tidak pernah masuk image, tidak pernah masuk git.
- Image Docker = runtime (PHP/FrankenPHP/ext); tidak ada git/token/secret.
- **Source di-BIND-MOUNT** ke `/app` container (BUKAN di-COPY ke image) -> update
  kode langsung live via `opcache.validate_timestamps=1`, tanpa rebuild image.
  `vendor/` & `public/build` hidup di `/home/<proj>/src/`; `storage` &
  `bootstrap/cache` = named volume (persisten, tak ketiban bind-mount).

---

## 2. Checklist preventif (LAKUKAN DULU sebelum deploy pertama)

Tiap item di bawah ini = satu error yang kita temui. Cek dari awal:

- [ ] **Semua file config ASCII-only.** Dockerfile, docker-compose.yml,
      deploy.sh, .env - JANGAN pakai em-dash (`—`), panah (`->` ok, `→` tidak),
      smart-quote (`"` `'`). Pakai ASCII biasa. File non-ASCII yang di-transfer
      Windows -> Linux bisa korup jadi `yaml: invalid leading UTF-8 octet`.
- [ ] **`.env` pakai LF line ending**, bukan CRLF. Editor Windows sering simpan
      CRLF -> token & nilai env terbaca dengan `\r` di ujung -> auth gagal.
- [ ] **Cek versi PHP di `composer.lock`.** Cari `"php": ">=8.x"` tertinggi,
      samakan dengan tag base image (`frankenphp:1-php8.4-alpine`).
- [ ] **Cek nama file config persis.** `vite.config.js` atau `.ts`?
      `eslint.config.js`? Dockerfile `COPY` harus sama persis dengan nama asli.
- [ ] **VITE_* vars di-pass sebagai build args.** Semua `import.meta.env.VITE_*`
      di-bake ke JS bundle saat `npm run build` - kalau tidak di-pass saat build,
      jadi `undefined` di production.
- [ ] **`APP_URL` = URL publik asli** (`http://<ip>:<port>`), bukan localhost.
      Salah -> redirect & asset URL mengarah ke localhost -> halaman blank.
- [ ] **Project name compose eksplisit** (`docker compose -p <proj>`). Kalau
      tidak, pindah lokasi compose file -> container lama tidak dikenali ->
      konflik nama container.
- [ ] **Cek port server yang dipakai bebas.** `ss -tlnp | grep ':<port> '`.
- [ ] **Port di `${APP_BIND:-127.0.0.1}`**, port database hanya `127.0.0.1`: Docker melewati ufw.
- [ ] **`<Dockerfile>.dockerignore` allow-list** di sebelah Dockerfile: data privat yang di-gitignore tidak
      masuk build context.
- [ ] **Action di-pin ke commit sha, image ke digest** (`alpine/git`, `caddy`, base image).
- [ ] **Health endpoint di luar middleware host-check/auth**; `SMOKE_PATH` untuk membuktikan jalur database.
- [ ] **Token GitHub sendirian di `.deploy-token` (600)**, `.env` 600, folder deploy milik root
      (`server-setup.sh`).
- [ ] **(Filament) User model implement `FilamentUser` interface.** Di
      `APP_ENV=production`, tanpa ini SEMUA user akan dapat 403 setelah
      login (termasuk super_admin). Lihat E18.

---

## 3. Katalog error & fix

### E1. `yaml: invalid leading UTF-8 octet`

- **Gejala**: `docker compose build` gagal sebelum build mulai.
- **Sebab**: file `docker-compose.yml` punya byte non-UTF-8 (em-dash, smart
  quote di komentar) yang korup saat transfer dari Windows.
- **Fix cepat di server**:
  ```bash
  tr -cd '\11\12\15\40-\176' < docker-compose.yml > /tmp/c.yml && mv /tmp/c.yml docker-compose.yml
  ```
- **Cegah**: tulis semua file config pakai ASCII murni.

### E2. `Authentication failed for github.com` saat git clone/fetch

- **Gejala**: `remote: Invalid username or token` atau `could not read Username`.
- **Sebab A**: file token (`/root/<proj>/.deploy-token`) punya CRLF (deploy.sh sekarang membuang `\r`) -> token terbaca `ghp_xxx\r`.
- **Sebab B**: auth `http.extraHeader=Authorization: Bearer <token>` ditolak
  GitHub untuk operasi git (beda dengan API).
- **Fix**:
  ```bash
  sed -i 's/\r$//' /root/<proj>/.deploy-token   # buang CRLF (sama untuk .env)
  ```
  Untuk auth git, pakai credential helper yang membaca token dari env (username `x-access-token`), seperti di
  deploy.sh: token tidak masuk URL, argv (`ps`) maupun `.git/config`. Token inline di URL juga jalan, tapi
  terlihat di daftar proses.
- **Verifikasi token**: `curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer <TOKEN>" https://api.github.com/repos/<owner>/<repo>` harus `200`.

### E3. `"/repo/vite.config.ts": not found`

- **Sebab**: Dockerfile `COPY vite.config.ts` tapi file aslinya `vite.config.js`.
- **Fix**: samakan nama file di Dockerfile dengan file asli di repo.
- **Cegah**: `ls` dulu file config sebelum tulis Dockerfile.

### E4. `composer install`: `ext-gd is missing from your system`

- **Sebab**: image `composer:2` tidak punya PHP extension (gd, intl, dll) yang
  diminta package. Stage ini cuma untuk download dependency.
- **Fix**: tambah `--ignore-platform-reqs` di `composer install` stage vendor.
  Extension yang asli ada di runtime image (stage app).

### E5. `composer: not found` saat `dump-autoload`

- **Sebab**: runtime image (FrankenPHP) tidak punya composer.
- **Fix**: copy binary composer dari image composer:
  ```dockerfile
  COPY --from=composer:2 /usr/bin/composer /usr/bin/composer
  ```

### E6. `Composer detected issues in your platform: PHP >= 8.x`

- **Sebab**: `composer.lock` butuh PHP versi tertentu, base image lebih rendah.
- **Fix**: ganti tag base image sesuai (`frankenphp:1-php8.4-alpine`).
- **Cegah**: cek `composer.lock` dulu (lihat checklist).

### E7. `Conflict. The container name "/<x>" is already in use`

- **Sebab**: docker compose menentukan "project" dari nama folder compose file.
  Pindah lokasi compose file -> project beda -> container lama tidak dikenali.
- **Fix**: project name eksplisit di semua perintah compose: `-p <proj>`.
  Sekali konflik, hapus manual: `docker rm -f <container1> <container2> ...`.

### E8. Halaman blank, console: `You must pass your app key when you instantiate Pusher`

- **Sebab**: `VITE_*` env var (mis. `VITE_REVERB_APP_KEY`) tidak tersedia saat
  `npm run build` di Docker -> di-bake jadi `undefined` di JS bundle.
- **Fix**: pass sebagai build args + deklarasi ARG/ENV di stage frontend:
  ```dockerfile
  ARG VITE_REVERB_APP_KEY
  ENV VITE_REVERB_APP_KEY=$VITE_REVERB_APP_KEY
  ```
  ```yaml
  build:
    args:
      VITE_REVERB_APP_KEY: ${REVERB_APP_KEY}
  ```
- **Catatan**: ganti VITE_* berarti WAJIB rebuild (bukan cuma restart).

### E9. Halaman redirect ke `localhost`, bukan IP server

- **Sebab**: `APP_URL` di-set `http://localhost:<port>`.
- **Fix**: `.env` -> `APP_URL=http://<ip-server>:<port>`, lalu restart container
  (config Laravel di-cache saat boot).

### E10. Data basi setelah mutation (Inertia prefetch cache)

- **Gejala**: toggle/rename data, pindah page, balik lagi - status masih lama,
  baru benar setelah hard-refresh.
- **Sebab**: handler flush cache pakai event `success` yang TIDAK punya
  `event.detail.visit` (cuma `finish` yang punya). Handler crash, `flushAll()`
  tidak pernah jalan.
- **Fix** di `app.tsx`:
  ```js
  router.on('finish', (event) => {
      const method = event.detail.visit?.method?.toLowerCase();
      if (method && method !== 'get') router.flushAll();
  });
  ```

### E11. BuildKit secret `environment:` tidak terbaca

- **Sebab**: `secrets: { x: { environment: VAR } }` di compose kadang tidak
  forward nilai dari `--env-file` ke BuildKit (flaky antar versi).
- **Fix**: pakai file-based secret (`file: /path/to/token`), atau lebih simpel
  token inline di URL git (lihat E2).

### E12. Filament v4 + Octane = blank page tiap navigasi

- **Gejala**: First request setelah container restart -> works. Subsequent
  request ke `/shield/roles`, `/admin/<resource>`, dst -> blank page (200
  status, HTML returned tapi browser render kosong / hanya layout).
  Restart container -> first request kerja lagi -> loop.
- **Sebab**: State leak antar request di Octane worker mode untuk stack:
    - **Spatie Permission cache** poisoning antar user
    - **Filament Panel Manager** static registry corrupt
    - **Livewire 3 component snapshot** mount silent fail
    - **Auth manager singleton** cache user salah
- **Fix definitif**: SWITCH dari Octane ke plain frankenphp php-server.
  Per-request fresh boot, no state leak, masih 2-3x lebih cepat dari PHP-FPM.
  ```dockerfile
  # GANTI:
  CMD ["php", "artisan", "octane:start", "--server=frankenphp", ...]
  # JADI:
  CMD ["frankenphp", "php-server", "--listen", ":8080", "--root", "/app/public"]
  ```
- **Trade-off**: lose worker memory persistence (Laravel boot ~100-200ms per
  request). Untuk admin panel internal traffic moderate (<200 req/s), trade-off
  worth it untuk reliability.
- **Catatan**: Inertia + Octane jalan stabil (stateless response). Issue ini
  spesifik untuk Livewire/Filament stateful response.

### E13. AdminUserSeeder stuck saat `Artisan::call('shield:generate')`

- **Gejala**: `php artisan db:seed` muncul `AdminUserSeeder RUNNING` lalu
  hang berjam-jam. Manual `shield:generate` dari CLI cepat selesai.
- **Sebab**: `Artisan::call()` di dalam seeder tidak pass `--no-interaction`
  default, jadi command nunggu konfirmasi overwrite policy yang gak akan
  pernah datang.
- **Fix** di seeder (lihat E19 untuk `--option=permissions`):
  ```php
  Artisan::call('shield:generate', [
      '--all' => true,
      '--panel' => 'admin',
      '--option' => 'permissions',   // jangan overwrite Policy — lihat E19
      '--no-interaction' => true,
  ]);
  Artisan::call('shield:super-admin', [
      '--user' => $user->id,
      '--panel' => 'admin',
      '--no-interaction' => true,
  ]);
  ```

### E14. Passport migration FK error `oauth_clients` doesn't exist

- **Gejala**: `migrate` fail di custom migration yang FK ke `oauth_clients`
  (Passport). Error: "Failed to open the referenced table 'oauth_clients'".
- **Sebab**: Passport v13+ migrations tidak auto-load lagi dari vendor.
  Harus publish dulu. Setelah publish, default timestamp = HARI INI -> jalan
  SETELAH custom migration yang sudah ada -> FK gagal.
- **Fix**:
  1. Publish migrations:
     `php artisan vendor:publish --tag=passport-migrations`
  2. Rename file yang baru di-publish ke timestamp original (2016/2024) supaya
     jalan duluan:
     ```
     2016_06_01_000001_create_oauth_auth_codes_table.php
     2016_06_01_000002_create_oauth_access_tokens_table.php
     2016_06_01_000003_create_oauth_refresh_tokens_table.php
     2016_06_01_000004_create_oauth_clients_table.php
     2024_06_01_000001_create_oauth_device_codes_table.php
     ```
  3. Run `migrate:fresh --seed`

### E15. Redis PECL install gagal di alpine

- **Gejala**: `install-php-extensions redis` gagal. Errors:
  "No releases available for package pecl.php.net/redis" atau compilation
  error di Makefile.
- **Sebab**: `install-php-extensions redis` (tanpa versi) cari `latest` dari
  PECL yang kadang gak respon, atau versi terbaru punya kompat issue dengan
  alpine GCC.
- **Fix**: pin versi spesifik:
  ```dockerfile
  RUN install-php-extensions pcntl pdo_mysql ... redis-6.3.0 ...
  ```

### E16. Nginx reverse proxy buang port dari Host header

- **Gejala**: APP_URL set `http://ip:24000` tapi Laravel redirect ke
  `http://ip/login` (tanpa port). Browser nyasar ke port 80 yang gak ada
  server-nya.
- **Sebab**: nginx default pakai `proxy_set_header Host $host` yang gak
  include port. Laravel terima Host tanpa port, generate URL salah.
- **Fix**: pakai `$http_host` (preserve port):
  ```nginx
  proxy_set_header Host $http_host;
  proxy_set_header X-Forwarded-Host $http_host;
  proxy_set_header X-Forwarded-Port $server_port;
  ```
- **Better fix**: BUANG nginx kalau pakai FrankenPHP. FrankenPHP HTTP server
  langsung, gak butuh reverse proxy.

### E17. Filament asset URL `localhost:8080` di production

- **Gejala**: Console error 404 untuk `http://localhost:8080/light.png`
  (atau asset lain), bukan ke APP_URL yang benar. Page rendering broken.
- **Sebab**: `asset()` di-evaluate sekali di service provider boot. Worker
  mode (Octane) cache hasil. Atau config cache lama dengan APP_URL salah.
- **Fix**:
  1. Pastikan APP_URL benar di `.env` (include port kalau non-80).
  2. `php artisan config:cache` lalu restart container.
  3. Kalau pakai Octane (atau cached panel boot), bungkus dengan Closure
     supaya evaluate per request:
     ```php
     ->brandLogo(fn () => asset('light.png'))
     ->darkModeBrandLogo(fn () => asset('dark.png'))
     ```

### E18. Filament 403 "FORBIDDEN" setelah login di production env

- **Gejala**:
  - Login page (`/login`) tampil OK, status 200.
  - Auth credentials valid → form submit success.
  - Setelah redirect ke panel home (`/` atau `/admin`) → halaman dark
    minimal "403 | FORBIDDEN" (Laravel `errors::minimal` view).
  - Berlaku untuk SEMUA user, termasuk super_admin yang punya semua
    permission Shield.
  - `laravel.log` kosong (tidak ada exception ter-log).
  - `curl` tanpa auth ke `/` → 302 redirect ke `/login` (server-level
    routing OK).
  - Tidak terjadi saat dev (APP_ENV=local).
- **Sebab**: Filament v3/v4 punya behavior berbeda berdasarkan APP_ENV:
  - `local` → semua user authenticated otomatis boleh akses panel
    (convenience untuk dev).
  - `production` → User model **WAJIB** implement `FilamentUser`
    interface dengan `canAccessPanel()` method. Tanpa itu, abort(403)
    untuk semua request ke panel route (termasuk dashboard root).
  - Bug ini "tersembunyi" di dev karena Filament lenient di local mode.
    Migrasi local → production langsung break panel access tanpa warning.
- **Fix** — tambah `FilamentUser` ke `app/Models/User.php`:
  ```php
  use Filament\Models\Contracts\FilamentUser;
  use Filament\Panel;

  class User extends Authenticatable implements FilamentUser
  {
      public function canAccessPanel(Panel $panel): bool
      {
          // Minimal: izinkan semua user authenticated, biar Shield permission
          // yang gating akses per-resource.
          return true;

          // Atau ketat: cuma role tertentu.
          // return $this->hasAnyRole(['super_admin', 'admin']);
      }

      // ...rest of class
  }
  ```
- **Cegah**:
  - WAJIB User model implement `FilamentUser` SEBELUM first deploy ke
    production, bahkan kalau di dev pakai APP_ENV=local.
  - Test full login flow (klik login → masuk panel → buka resource) di
    APP_ENV=production locally **sebelum** push ke server. Switch
    sementara di `.env` lokal: `APP_ENV=production` + `php artisan config:clear`.
  - Kalau Pengaju login butuh role-based access ke panel, encode di
    `canAccessPanel` — jangan hardcode di middleware terpisah.

### E19. `shield:generate --all` overwrite policy file dengan permission name salah

- **Gejala**:
  - Setelah deploy, menu resource hilang untuk user yang seharusnya
    punya akses (mis. role `kasubbid` tidak lihat menu Pengajuan Sistem).
  - Super_admin tetap lihat semua (karena policy `Gate::before` bypass).
  - Permission row di tabel `permissions` ada dan benar.
  - User punya permission yang dimaksud (cek `$user->permissions`).
  - Tapi `auth()->user()->can('view_any_system::request')` → `false`.
  - File `app/Policies/SystemRequestPolicy.php` cek permission name
    yang SALAH — bukan `view_any_system::request` tapi mis.
    `view_any_system::request::approval`.
- **Sebab**: Default `php artisan shield:generate --all` regenerate
  TIGA hal: row permission di DB, file Policy, dan registrasi di
  `AuthServiceProvider`. Saat ada **2 atau lebih Filament Resource yang
  share model yang sama** (mis. `SystemRequestResource` + `SystemRequestApprovalResource`
  → keduanya `protected static ?string $model = SystemRequest::class`),
  Shield iterate semua resource dan **resource yang last-iterated menang**.
  Policy file di-overwrite dengan nama permission dari resource terakhir,
  sehingga policy untuk model yang sama check permission dari resource lain.
- **Fix** — pakai flag `--option=permissions` supaya cuma generate row
  permission di DB, tidak overwrite policy file:
  ```sh
  php artisan shield:generate --all --panel=admin --option=permissions --no-interaction
  ```
  Di `entrypoint.sh`:
  ```sh
  if [ "${RUN_SHIELD:-false}" = "true" ]; then
      php artisan shield:generate --all --panel=admin --option=permissions --no-interaction 2>/dev/null \
          || php artisan shield:generate --all --panel=admin --no-interaction \
          || true
  fi
  ```
  Fallback ke versi tanpa `--option` untuk versi Shield lama yang
  belum support flag itu.
- **Cegah**:
  - **Hindari multiple resource untuk model yang sama** kalau bisa —
    pakai 1 resource + custom Page/Action untuk workflow berbeda.
  - Kalau memang butuh multi-resource ke 1 model (mis. untuk split nav
    group berdasarkan stage workflow), commit file Policy ke git dan
    JANGAN biarkan Shield overwrite di production.
  - Generate ulang manual saat develop saja (`shield:generate --resource=Foo`),
    bukan di entrypoint container.
  - Verifikasi nama permission di Policy = nama permission yang di-check
    di Resource (`canViewAny` static method).

### E20. App stuck di maintenance mode setelah migration fail mid-deploy

- **Gejala**: Browser tampil halaman "503 Service Unavailable" Laravel
  default. App gak balik normal sampai container restart manual.
- **Sebab**: deploy script jalanin `artisan down --retry=60` di awal
  (maintenance mode ON). Migration di tengah gagal (mis. table sudah ada,
  FK error). Script `set -e` exit -> `artisan up` di akhir gak ke-execute
  -> app stuck maintenance.
- **Fix sementara** (server):
  ```bash
  docker exec <app-container> php /app/artisan up
  ```
- **Cegah** di entrypoint.sh: JANGAN pake `artisan down`/`artisan up`
  pattern di deploy. Pakai pattern blue-green atau biarkan container baru
  langsung serve traffic (downtime ~5-10 detik saat compose recreate).
  ```sh
  # YANG SALAH (legacy pattern):
  artisan down --retry=60
  # ... migrate, seed, cache ...
  artisan up
  
  # YANG BENAR: entrypoint.sh langsung migrate + serve,
  # docker compose up -d handle replace container atomik.
  ```

### E21. 502 Bad Gateway: FrankenPHP "Cannot find application base path"

- **Gejala**: nginx/proxy depan return 502. Container `app` running tapi
  log FrankenPHP berulang-ulang:
  ```
  Cannot find application base path.
  failed to initialize workers: too many consecutive failures
  ```
- **Sebab**: Octane worker script (`vendor/laravel/octane/bin/bootstrap.php`)
  baca `$_SERVER['APP_BASE_PATH']`. Tanpa env ini diset, basePath null,
  worker exit code 11 sebelum reach `frankenphp_handle_request()`.
- **Fix kalau pakai Octane**:
  ```dockerfile
  ENV APP_BASE_PATH=/app
  ```
- **Fix lebih baik**: switch ke plain `frankenphp php-server` mode (lihat
  E12) - gak butuh APP_BASE_PATH sama sekali, gak ada worker script.

### E22. `.git/FETCH_HEAD: Permission denied` saat `git pull` di host

- **Gejala**: User `bapenda` jalanin `git pull` di repo source, dapat:
  ```
  error: cannot open '.git/FETCH_HEAD': Permission denied
  ```
- **Sebab**: Sebelumnya pernah jalanin `sudo bash deploy.sh` atau git
  operation lain dengan sudo -> file `.git/*` jadi milik root. User
  regular gak punya write access.
- **Fix permanen** (sekali setting):
  ```bash
  sudo chown -R bapenda:bapenda /home/bapenda/<project>/
  sudo usermod -aG docker bapenda
  newgrp docker   # apply group baru tanpa logout
  ```
- **Cegah**: pakai deploy.sh template (alpine/git container) - user
  bapenda gak perlu sudo lagi untuk deploy, semua jalan via docker.

### E23. Container name conflict saat migrasi naming convention

- **Gejala**: `docker compose up -d` error:
  ```
  Conflict. The container name "/redis" is already in use by container ...
  ```
- **Sebab**: Compose lama pakai container name `phpfoo-redis`, compose
  baru pakai `foo-redis` (atau service name `redis`). Compose detect
  service "redis" lain di docker daemon tapi gak track sebagai project
  yang dia kelola -> conflict.
- **Fix**:
  ```bash
  # 1. Stop & remove SEMUA container lama dengan naming lama
  docker rm -f phpfoo-app phpfoo-redis phpfoo-nginx phpfoo-composer 2>/dev/null
  
  # 2. Atau bersih total project lama:
  cd /docker/foo-old/
  docker compose -p foo-old down --remove-orphans -v
  
  # 3. Lalu up project baru
  cd /docker/foo/
  docker compose -p foo up -d
  ```
- **Cegah**: kalau ganti naming convention di compose, **WAJIB** rm
  container lama dulu. `docker compose up -d` gak auto-cleanup container
  dengan nama beda.

### E25. Build error: `Class "X\ServiceProvider" not found` setelah remove package

- **Gejala**: Setelah `composer remove <package>`, `docker compose build`
  gagal di stage `composer dump-autoload`:
  ```
  In ProviderRepository.php line 205:
  Class "Laravel\Octane\OctaneServiceProvider" not found
  Script @php artisan package:discover handling the post-autoload-dump event returned with error code 1
  ```
- **Sebab**: `bootstrap/cache/services.php` atau `bootstrap/cache/packages.php`
  lokal di host masih punya cached reference ke package yang sudah di-remove.
  Tanpa `.dockerignore` yang exclude `bootstrap/cache/`, file ini ikut
  `COPY . .` ke image. Vendor sudah clean (composer install gak install
  package itu) tapi cache file stale.
- **Fix**: tambah `.dockerignore` di root project:
  ```
  bootstrap/cache/*.php
  vendor
  node_modules
  .env
  .git
  ```
- **Catatan**: cache di-regenerate di `entrypoint.sh` via
  `config:cache`/`route:cache`/`view:cache`, jadi gak hilang functionality.

### E24. Folder `/docker/<proj>/` masih punya file/folder config lama

- **Gejala**: Setelah refactor structure (mis. dari supervisor+nginx ke
  plain FrankenPHP), folder `/docker/<proj>/` masih ada `nginx/`,
  `supervisor/`, `frankenphp/Caddyfile`, `Dockerfile` lama. Deploy yg
  baru gak hapus file-file lama, cuma OVERWRITE yang nama-nya sama.
- **Sebab**: Sync logic di deploy.sh pakai `cp` (file-by-file) atau
  `rsync` tanpa `--delete` flag. File lama yang sudah dihapus di repo
  tetap nyangkut di server.
- **Fix manual** (sekali saat refactor):
  ```bash
  # Backup dulu (1 file)
  tar czf /tmp/old-config-$(date +%s).tar.gz /docker/<proj>/
  
  # Wipe semua kecuali deploy.sh
  cd /docker/<proj>/
  ls | grep -v '^deploy.sh$' | xargs -r rm -rf
  
  # Copy deploy.sh terbaru dari source
  cp /home/<user>/<proj>/src/docker/deploy.sh .
  chmod +x deploy.sh
  
  # Run deploy - script auto-copy file baru
  ./deploy.sh
  ```
- **Cegah** di deploy.sh: pakai `rsync -a --delete` (tapi hati-hati,
  hapus file yang gak ada di source).

### E26. Mixed Content: asset CSS/JS HTTP padahal page HTTPS (di belakang reverse proxy SSL)

- **Gejala**:
  - Setelah kasih domain + sertifikat SSL ke app (mis. dari `http://<ip>:<port>`
    ke `https://app.domain.go.id`), halaman login kelihatan tanpa style.
  - Console browser penuh error:
    ```
    Mixed Content: The page at 'https://app.domain.go.id/login' was loaded over HTTPS,
    but requested an insecure stylesheet 'http://app.domain.go.id/css/filament/...'.
    This request has been blocked.
    ```
  - Pengamatan: page-nya HTTPS, tapi semua `<link>` & `<script>` src-nya HTTP.
- **Sebab**: Reverse proxy (nginx/apache/haproxy) terminate SSL lalu forward
  HTTP biasa ke container FrankenPHP/PHP-FPM. Laravel di container baca
  request sebagai HTTP (tidak tau ada SSL di depan). Helper `asset()` /
  `url()` generate URL dengan scheme `http://` → browser block sebagai
  mixed content.
- **Fix** — kombo 3 hal, semua wajib:

  1. **Trust proxy** di `bootstrap/app.php` (Laravel 11+):
     ```php
     use Illuminate\Http\Request;

     ->withMiddleware(function (Middleware $middleware) {
         $middleware->trustProxies(
             at: '*',  // atau IP spesifik proxy kalau mau ketat
             headers: Request::HEADER_X_FORWARDED_FOR
                 | Request::HEADER_X_FORWARDED_HOST
                 | Request::HEADER_X_FORWARDED_PORT
                 | Request::HEADER_X_FORWARDED_PROTO
                 | Request::HEADER_X_FORWARDED_AWS_ELB,
         );
     })
     ```
     Untuk Laravel 10-, edit `app/Http/Middleware/TrustProxies.php`:
     `protected $proxies = '*';`

  2. **Force HTTPS** di production (belt-and-suspenders, kalau proxy lupa
     set X-Forwarded-Proto). Di `AppServiceProvider::boot()`:
     ```php
     use Illuminate\Support\Facades\URL;

     if ($this->app->environment('production')) {
         URL::forceScheme('https');
     }
     ```

  3. **`APP_URL`** di `.env` server pakai `https://` + domain:
     ```env
     APP_URL=https://app.domain.go.id
     ```
     Lalu restart container (`config:cache` di entrypoint baca env baru).

- **Cegah**:
  - SEBELUM kasih domain HTTPS, pastikan trustProxies + forceScheme udah
    di-setup di code, dan APP_URL siap diganti di `.env`.
  - Pastikan proxy depan (nginx, dll) set header `X-Forwarded-Proto $scheme;`
    di config reverse proxy-nya. Contoh nginx:
    ```nginx
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Host  $host;
    proxy_set_header X-Forwarded-Port  $server_port;
    ```
  - Verifikasi setelah deploy: `curl -I https://domain/login | grep Location`
    — kalau ada redirect ke `http://`, berarti scheme belum bener.

### E27. Container error: `network xxx not found` saat restart / recreate

- **Gejala**:
  ```
  failed to set up container networking: network e589aee2a4aa...
  not found
  ```
  Container tidak bisa start / restart karena network yang dia referensikan
  sudah tidak ada.
- **Sebab**: Compose file yang naming / struktur network-nya berubah antar
  deploy (mis. rename network, split service ke network berbeda, hapus
  eksplisit `networks:` block). Container config masih menyimpan reference
  ke network ID lama, tapi network itu sudah di-hapus atau di-recreate
  dengan ID berbeda. `docker compose up -d --remove-orphans` **tidak**
  hapus network — cuma hapus orphan container. Jadi network dari versi
  compose lama menumpuk.
- **Diagnosis**:
  ```bash
  # Cek semua network yang berlabel project ini
  docker network ls --filter "label=com.docker.compose.project=<proj>" \
    --format '{{.Name}}\t{{.ID}}'

  # Inspect network mode container yang gagal
  docker inspect <container-id> --format '{{.HostConfig.NetworkMode}}'
  ```
- **Fix satu kali**:
  ```bash
  # 1. Down project (biar container tidak nyantol ke network lama)
  docker compose -p <proj> down --remove-orphans

  # 2. Hapus semua network project yang orphan (aman: docker network rm
  #    fail untuk network yang masih dipakai)
  docker network ls --filter "label=com.docker.compose.project=<proj>" \
    --format '{{.Name}}' \
    | while read net; do docker network rm "$net" 2>/dev/null || true; done

  # 3. Bersihkan container broken (opsional)
  docker rm -f <container1> <container2> 2>/dev/null

  # 4. Redeploy (compose bikin network baru + container baru)
  ./deploy.sh --no-fetch
  ```
- **Cegah**: sebelum refactor network structure di compose (rename
  service/network, split ke network beda, dll), **audit dulu**:
  ```bash
  docker ps --filter label=com.docker.compose.project=<proj>
  docker network ls --filter label=com.docker.compose.project=<proj>
  docker volume ls --filter label=com.docker.compose.project=<proj>
  ```
  Kalau ada leftover dari eksperimen sebelumnya, bersihkan dulu sebelum
  deploy versi baru. `deploy.sh` **tidak auto-cleanup network** by design
  — karena bisa nabrak race condition kalau container sedang start dan
  logic-nya cari network yang mau dihapus.
- **Alternatif periodik**: `docker network prune -f` hapus SEMUA network
  yang tidak dipakai container manapun (aman, tapi lintas project).

---

### E28. Kode baru tidak kepakai di HTTP padahal CLI sudah benar (opcache beku)

- **Gejala** (membingungkan karena CLI dan HTTP tidak sinkron):
  - `php artisan route:list` **menampilkan** route baru, tapi request HTTP
    balas **404**.
  - Menu/page Filament baru **tidak muncul** di sidebar walau
    `canAccess()` dan `getNavigationItems()` sudah benar saat dicek via
    tinker.
  - Tombol/fitur yang **sudah dihapus** dari kode masih tampil di browser.
  - Sempat "sembuh" setelah clear cache manual, lalu **kambuh lagi** tiap
    deploy berikutnya.
- **Sebab**: mode bind-mount mengandalkan `opcache.validate_timestamps=1`
  supaya OPcache mengecek mtime file dan me-refresh kode tanpa restart.
  Kalau **image ter-build sebelum** `php/opcache.ini` disetel (atau
  setelannya ke-override), nilainya `Off` → OPcache **tidak pernah**
  mengecek perubahan file → proses HTTP menyajikan kode yang beku sejak
  image dibuat. CLI tidak terpengaruh karena `opcache.enable_cli=Off`,
  jadi `artisan` selalu terlihat benar. Container yang `Up 4 weeks`
  memperparah: tidak pernah ada restart yang bisa me-reset OPcache.
- **Diagnosis**:
  ```bash
  # Cek setelan yang AKTIF di runtime (bukan isi file .ini)
  docker compose -p <proj> exec app php -i \
    | grep -iE "opcache.enable|validate_timestamps|revalidate_freq"
  ```
  `opcache.validate_timestamps => Off` = terkonfirmasi.

  Uji silang CLI vs HTTP — kalau hasilnya beda, hampir pasti opcache:
  ```bash
  # File di disk sudah bersih?
  docker compose -p <proj> exec app grep -c "kode_lama" app/Path/File.php
  # Tapi browser masih menampilkan yang lama -> opcache beku
  ```
- **Fix cepat**:
  ```bash
  docker compose -p <proj> restart app
  ```
- **Fix permanen** (biar tidak berulang): rebuild image supaya
  `php/opcache.ini` terbaru ikut ter-copy.
  ```bash
  ./deploy.sh --build
  ```
  Verifikasi setelahnya harus `validate_timestamps => On`.
- **Guard di deploy.sh**: script sekarang membaca setelan opcache runtime
  tiap deploy dan **restart app otomatis hanya kalau `validate_timestamps`
  = Off**. Jadi walau image drift, deploy tetap menyajikan kode baru
  (dengan trade-off downtime ±13 detik).
- **Catatan desain**: jangan set `validate_timestamps=0` selama pakai
  bind-mount. Setelan itu hanya benar untuk pola COPY-source-in-image,
  di mana kode memang tidak berubah selama container hidup.

---

### E29. Page/menu Filament baru 404 setelah deploy (urutan cache salah)

- **Gejala**: page Filament baru (Resource/Page) tidak muncul di sidebar,
  URL-nya 404, dan permission-nya tidak ada di halaman Roles — padahal
  file sudah ada di server dan class-nya ter-load.
- **Sebab**: `php artisan optimize:clear` **tidak** membersihkan cache
  komponen Filament (`bootstrap/cache/filament/`). Itu perintah Laravel
  core (route/config/view/event) saja. Kalau urutannya:
  ```
  optimize:clear -> config:cache -> route:cache -> ... -> filament:optimize
                                    ^^^^^^^^^^^
  ```
  maka saat `route:cache` berjalan, Filament masih membaca **daftar
  komponen lama** → route panel untuk page baru tidak ikut ter-cache →
  404. `filament:optimize` di akhir sudah terlambat: route cache terlanjur
  ditulis tanpa route tersebut.

  Efek samping: `shield:generate` yang jalan sebelum cache Filament
  di-clear juga membaca daftar lama → permission page baru tidak pernah
  dibuat → tidak muncul di halaman Roles.
- **Diagnosis**:
  ```bash
  # Cache Filament basi? Bandingkan tanggalnya dengan file cache lain
  docker compose -p <proj> exec app ls -la bootstrap/cache/
  # folder `filament` bertanggal jauh lebih lama = stale
  ```
- **Urutan yang benar** (sudah diterapkan di `deploy.sh`):
  ```
  optimize:clear
  filament:optimize-clear    <- WAJIB, optimize:clear tidak menyentuh ini
  filament:optimize          <- re-discover page/resource
  config:cache
  route:cache                <- kini memuat route page baru
  view:cache
  event:cache
  ```
  Dan `shield:generate` didahului `filament:optimize-clear`.
- **Fix manual kalau cache terlanjur basi**:
  ```bash
  docker compose -p <proj> exec app rm -rf bootstrap/cache/filament
  docker compose -p <proj> exec app php artisan filament:optimize
  docker compose -p <proj> exec app php artisan route:cache
  ```

---

### E30. Cache di-generate container one-off, tidak konsisten dengan app

- **Gejala**: menu/halaman muncul-hilang antar refresh; hasil `artisan`
  tidak konsisten dengan yang dilihat browser.
- **Sebab**: perintah cache dijalankan lewat
  `docker compose run --rm` (container **one-off**). Meski
  `bootstrap/cache` adalah named volume yang di-share, container one-off
  punya OPcache sendiri — cache yang ditulis di sana bisa tidak konsisten
  dengan container app yang sedang melayani request.
- **Fix**: jalankan artisan lewat `exec` ke container yang **running**.
  `deploy.sh` menyediakan helper:
  ```bash
  app_artisan() {
      if $COMPOSE ps --status running --services | grep -qx "$APP_SERVICE"; then
          $COMPOSE exec -T "$APP_SERVICE" php artisan "$@"
      else
          $RUN --entrypoint php "$APP_SERVICE" artisan "$@"   # fallback first deploy
      fi
  }
  ```
  Dipakai untuk: `migrate`, `shield:generate`, `permission:cache-reset`,
  `optimize:clear`, `config/route/view/event:cache`, `filament:optimize`,
  `queue:restart`.
- **Tetap pakai `run` (benar)**: `composer install` (jalan sebelum
  `up -d`) dan operasi file murni seperti `touch` marker.

---

## 4. Gotcha aplikasi (bukan deploy, tapi muncul saat testing deploy)

### Radix Select/Popover di dalam Dialog menutup Dialog

- **Sebab**: Popover/Select render lewat portal di luar DOM Dialog. Klik opsi
  dianggap "klik luar" -> Dialog auto-close.
- **Fix**: guard di `DialogContent`:
  ```jsx
  onInteractOutside={(e) => {
      if ((e.target as Element)?.closest('[data-radix-popper-content-wrapper]'))
          e.preventDefault();
  }}
  ```

### Seeder tidak idempotent

- Seeder yang dijalankan tiap deploy WAJIB pakai `firstOrCreate` /
  `updateOrCreate` - bukan `create()` (duplikat / error unique constraint).
- **Hati-hati**: `updateOrCreate` pada user/admin akan **reset password** tiap
  run. Pakai `firstOrCreate` supaya record existing tidak ditimpa.
- `db:seed` TIDAK punya riwayat - tiap dipanggil, SEMUA seeder jalan ulang
  penuh (beda dengan `migrate` yang track tabel `migrations`).

---

## 5. Migrate & seed otomatis tiap deploy

Taruh di `entrypoint.sh` (jalan tiap container start), gate dengan flag supaya
sidecar (scheduler/queue/reverb) tidak ikut menjalankan:

```sh
if [ "${RUN_MIGRATIONS:-true}" = "true" ]; then
    php artisan migrate --force --no-interaction || true
    php artisan db:seed  --force --no-interaction || true
fi
```

Service utama: `RUN_MIGRATIONS=true`. Sidecar: `RUN_MIGRATIONS=false`.
Syarat: semua seeder idempotent (lihat gotcha di atas).

> **Mode bind-mount:** update kode biasa TIDAK me-restart container app, jadi
> entrypoint tidak jalan lagi (migration entrypoint tak ter-trigger). Karena itu
> `deploy.sh` juga menjalankan `migrate --force` sendiri ketika ada file baru di
> `database/migrations/`. Migration tetap jalan walau container tidak recreate.

---

## 6. Alur kerja final

**Setup awal server (sekali):**

1. `server-setup.sh` (BOSS-GUIDE langkah 3), lalu upload `deploy.sh` ke `/root/<proj>/`, `chmod 755`, milik root.
2. Buat `/home/<proj>/src/.env` (copy dari `.env.example`), isi semua secret,
   `chmod 600`. Pastikan LF line ending. Token GitHub sendirian di `/root/<proj>/.deploy-token` (600).
3. `./deploy.sh` - pertama kali akan `git init` + clone source.

**Tiap update:**

1. Laptop: `git push origin main`.
2. Server: `/root/<proj>/deploy.sh`.

deploy.sh otomatis (dari git diff `.deploy-changed`): git sync -> self-update ->
sync docker config -> build/composer/vite **HANYA kalau file terkait berubah** ->
up -d -> migrate kalau ada migration baru -> refresh cache + `queue:restart`
(tanpa restart container). Update kode biasa = **tanpa rebuild**, kode live
<=2 dtk (bind-mount + opcache).

**Transisi pertama dari pola COPY-source-in-image:** opcache & autoload berubah
dan di-bake ke image, jadi deploy pertama WAJIB rebuild sekali. Jalankan
`deploy.sh` (self-update ke versi baru) lalu ulangi `deploy.sh --build`.

**Flag deploy.sh:**

- `--no-fetch` : skip git sync, pakai source yang ada.
- `--build`    : PAKSA rebuild image (transisi awal / recovery).
- `--no-build` : PAKSA skip rebuild image.
- `--logs`     : tail log app setelah deploy.

---

## 7. Verifikasi setelah deploy

```bash
# Container jalan semua?
docker compose -p <proj> ps

# HTML response benar (ada vite script, asset URL = IP server)?
curl -s -H "Host: <ip>:<port>" http://localhost:<port>/ | head -40

# Asset ter-build?
docker exec <proj>-app ls /app/public/build/

# Log ada error?
docker compose -p <proj> logs --tail 100 app

# Kode baru benar-benar kepakai? (bind-mount butuh validate_timestamps=On)
docker compose -p <proj> exec app php -i | grep -i validate_timestamps
# Off -> kode beku, lihat E28. Fix: ./deploy.sh --build

# Cache Filament fresh? (folder `filament` harus setanggal cache lain)
docker compose -p <proj> exec app ls -la bootstrap/cache/
# folder `filament` jauh lebih tua = stale, lihat E29
```

**Uji silang CLI vs HTTP** — kalau keduanya beda, penyebabnya hampir pasti
opcache beku (E28) atau cache Filament basi (E29):

```bash
# Route terdaftar menurut CLI?
docker compose -p <proj> exec app php artisan route:list --path=<path-baru>

# Tapi HTTP-nya bagaimana? (302 = OK/redirect login, 404 = bermasalah)
curl -s -o /dev/null -w "%{http_code}\n" http://<ip>:<port>/<path-baru>
```

Browser DevTools (F12):

- **Console**: error JS? (mis. Pusher app key -> E8)
- **Network**: ada asset 404? URL asset = IP server, bukan localhost?

---

## 8. Rotate / amankan token

- Token GitHub: pakai **fine-grained PAT**, scope `Contents: Read-only` pada
  repo spesifik. Bukan classic PAT (akses semua repo).
- Token sendirian di `/root/<proj>/.deploy-token` (600), tidak di `.env` (app bisa membaca `.env`-nya).
  Cuma dipakai container `alpine/git` saat sync, dioper lewat nama env dan dibaca credential helper:
  tidak ada di URL, argv, atau `.git/config`.
- Token TIDAK pernah masuk image Docker.
- `.env` di-`chmod 600`, tidak di-track git (`.gitignore`).
- Kalau token bocor: revoke di GitHub settings, generate baru, update `.deploy-token`,
  `./deploy.sh`.

---

## 9. Referensi file

Template siap pakai ada di kit ini:

- `deploy.sh`                   - script deploy generic (git sync + build + up)
- `references/laravel/react-inertia/` - Dockerfile + compose + entrypoint stack Inertia
- `filament/`                   - Dockerfile + compose + entrypoint stack Filament
- `SKILL.md`                    - cara pakai kit untuk project baru

Salin `deploy.sh` + isi folder stack yang sesuai ke folder `docker/` repo
project, lalu edit blok konfigurasi (lihat SKILL.md).

---

## 10. deploy.sh v2: zero-downtime + CI/CD (E31-E35)

Versi 2 (`deploy.sh` di skill ini) menutup titik downtime yang tersisa di v1.
Semua perilaku v1 (bind-mount, deteksi git diff, urutan cache E29, exec ke
container running E30, safety net opcache E28) tetap sama.

### E31. "Vite manifest not found" / halaman 500 selama build frontend

- **Gejala**: selama deploy yang mengubah `resources/`, halaman error beberapa
  detik sampai puluhan detik, lalu normal lagi.
- **Sebab**: v1 menjalankan `npm run build` langsung di source yang
  di-bind-mount. Vite mengosongkan `public/build` di awal build, jadi selama
  build berjalan manifest dan asset tidak ada.
- **Fix (v2)**: build di salinan source di container node, salin asset ber-hash
  baru ke `public/build` (tidak bentrok dengan yang lama), lalu ganti
  `manifest.json` dengan rename (atomik). Asset lama disimpan
  `KEEP_ASSET_DAYS` hari supaya tab browser lama tidak 404.
- **Terukur**: build 45 ms di proyek kecil: v1 gagal 4 dari 716 request,
  v2 gagal 0 dari 688. Build proyek nyata 20-60 detik, kegagalan v1 sebanding.

### E32. Error acak selama `composer install` saat deploy

- **Sebab**: v1 meng-install langsung ke `vendor/` yang sedang dipakai; request
  bisa memuat paket yang setengah ter-update.
- **Fix (v2)**: `cp -a vendor .vendor-new`, install dengan
  `COMPOSER_VENDOR_DIR=.vendor-new --no-scripts`, lalu
  `mv vendor .vendor-old && mv .vendor-new vendor` dan `package:discover`.
  Kedalaman folder sama dengan `vendor/`, jadi path relatif autoload tetap
  benar. Terukur: 0 dari 1088 request gagal.

### E33. Ganti `.env` harus recreate container (downtime 5-15 detik)

- **Sebab**: compose v1 memakai `env_file`. Variabel container MENANG atas file
  `.env` (Dotenv immutable), jadi `config:cache` tetap membaca nilai lama sampai
  container di-recreate.
- **Fix (v2)**: service app tanpa `env_file`; Laravel membaca `/app/.env`
  (bind-mount) langsung. deploy.sh menyimpan hash per key (bukan nilai) di
  `/root/<proj>/.env.deployed-keys`, tahu key mana yang berubah, lalu cukup
  refresh config. `VITE_*` berubah -> build frontend ulang. Key yang dipakai
  compose sendiri (port, build args) -> `up -d` recreate service itu saja.
- **Catatan**: kode yang memakai `getenv()` langsung (bukan `env()`/`config()`)
  tidak melihat isi `.env`. `entrypoint.sh` membaca `DB_HOST`/`DB_PORT` dari
  `.env` sendiri untuk menunggu database.
- **Transisi**: deploy pertama dengan compose baru me-recreate container sekali.

### E34. Koneksi websocket putus di setiap deploy

- **Sebab**: v1 selalu `restart scheduler reverb` di stack Octane.
- **Fix (v2)**: restart hanya kalau vendor, `config/` atau `.env` berubah
  (`LONG_RUNNING`). `schedule:work` menjalankan tiap jadwal di proses baru,
  jadi perubahan kode app ikut terpakai tanpa restart.

### E35. Deploy commit yang belum dites / dua deploy bersamaan

- **Sebab**: v1 selalu deploy HEAD branch saat itu; CI dan manual bisa jalan
  berbarengan.
- **Fix (v2)**: `--ref <sha>` (harus ada di BRANCH; dipakai workflow deploy
  untuk commit yang baru saja dites ulang), `flock` per project, health check
  `/up` di akhir (exit 2 kalau gagal), `--rollback` ke commit sebelum deploy
  terakhir (migration tidak di-undo).

### Masih ada (sengaja, jarang terjadi)

- Git sync menulis file di tempat (jendela di bawah 1 detik).
- Rebuild image (Dockerfile/entrypoint/php berubah) tetap recreate container.
  Zero-downtime penuh untuk kasus ini butuh reverse proxy + blue/green
  (mis. dua container app di belakang proxy); belum dipakai supaya stack
  tetap sederhana.
