# Panduan CI/CD untuk Bos (proyek web dengan deploy)

Agent dev (dengan skill web-deploy) merancang file deploy untuk setiap proyek nyata (Laravel, Rust, Go, dan lainnya) dari prinsip di `SKILL.md`. Alur yang kamu alami selalu sama, yang berbeda hanya isi folder `docker/`.

## 1. Apa bedanya dengan cara sekarang
Sekarang: kamu `git push`, lalu login ke server dan menjalankan `/root/<proj>/deploy.sh`.
Dengan CI/CD, langkah login ke server itu diambil alih GitHub:

```
agent selesai task -> task.mjs ship -> Pull Request
        |
        v
CI (GitHub Actions, ci.yml): composer install, npm build, php artisan test
        |  lulus -> merge ke main (otomatis; perubahan sensitif kamu review dulu)
        v
agent minta deploy -> PROMPT ke kamu (commit + daftar task)  <- satu-satunya approval deploy
        |  Approve
        v
deploy.yml: tes ulang commit itu -> server menjalankan deploy.sh --ref <commit>
        |
        v
deploy.sh v2: update tanpa restart untuk perubahan biasa, health check /up, catat rollback
```

- **CI** = continuous integration: setiap perubahan dites otomatis di GitHub sebelum di-merge.
- **CD** = continuous deployment: commit yang sudah lolos tes dipasang ke server oleh GitHub, bukan oleh kamu.
- Deploy **tidak** jalan otomatis di setiap merge. Kamu yang menyetujuinya: lewat prompt di Claude Code (`task.mjs deploy`) atau tombol **Run workflow** di tab Actions GitHub.
- Kenapa tidak memakai fitur "required reviewers" GitHub: untuk repo **private**, fitur itu hanya ada di paket Enterprise. Repo private gratis tidak punya. Jadi gerbangnya di framework ini.

## 2. Pilih cara GitHub menjangkau server (sekali per proyek)
| | Self-hosted runner (disarankan) | SSH |
|---|---|---|
| Cocok untuk | server kantor/intranet, tidak bisa diakses dari internet | server yang punya IP publik dan port SSH terbuka |
| Port yang dibuka | tidak ada (runner hanya keluar lewat HTTPS 443) | SSH masuk dari GitHub |
| Kunci rahasia | tidak ada kunci yang keluar dari server | deploy key disimpan di GitHub Secrets |
| Syarat | repo private (jangan pakai runner di repo publik) | host key server di-pin |

## 3. Setup server (sekali, dari folder kit seperti biasa)
1. **Bootstrap sekali** dengan `server-setup.sh` (agent tidak boleh membuat user, sudoers atau service di server, jadi ini langkahmu; perintah persisnya ada di kepala file itu): folder `/root/<proj>` dan `/home/<proj>/src` milik root, user runner **tanpa** grup `docker` (grup docker = root di mesin itu, tempat database produksi juga), satu baris sudoers untuk `deploy.sh`, file rahasia kosong chmod 600, dan runner GitHub kalau kamu memberi token registrasi (`gh api -X POST repos/<owner>/<repo>/actions/runners/registration-token -q .token`, berlaku 1 jam).
   - `.env`: proyek binary (Rust/Go) di `/root/<proj>/.env`; Laravel di `/home/<proj>/src/.env` (Laravel membacanya dari folder app). chmod 600, LF.
   - Token GitHub read-only (fine-grained, hanya repo ini, Contents: Read) disimpan **sendirian** di `/root/<proj>/.deploy-token` (chmod 600), tidak di `.env`, jadi tidak pernah sampai ke container aplikasi.
   - Isi keduanya lewat stdin ssh, jangan lewat chat atau command line: `ssh root@<server> "umask 077; cat > /root/<proj>/.deploy-token" < token.txt`.
   - Port: `APP_BIND` di `.env` = alamat yang dibuka user (IP LAN) atau `127.0.0.1` di belakang proxy. Docker melewati ufw, jadi alamat inilah batasnya.
2. **Transisi dari compose lama:** compose baru tidak lagi memakai `env_file`. Deploy pertama dengan file baru me-recreate container sekali (ada downtime singkat). Sesudah itu, perubahan `.env` tidak perlu restart lagi.
3. **Opsi runner:**
   - `server-setup.sh` dengan `RUNNER_TOKEN` dan `REPO` memasang runner sebagai service (label `deploy-<proj>`). Cara manual: GitHub repo -> Settings -> Actions -> Runners -> **New self-hosted runner** -> Linux, ikuti perintahnya; jalankan `./bin/installdependencies.sh` dari DALAM folder runner, lalu `sudo ./svc.sh install <user>` dan `sudo ./svc.sh start`.
   - Runner jalan dengan user biasa (bukan root, bukan anggota grup `docker`) yang hanya boleh menjalankan **satu** skrip sebagai root: `<user> ALL=(root) NOPASSWD: /root/<proj>/deploy.sh` di `/etc/sudoers.d/<proj>-deploy` (dibuat `server-setup.sh`, dicek `visudo`).
   - Sisa risiko yang perlu kamu tahu: runner menjalankan workflow dari branch MANA PUN yang menyebut labelnya, jadi branch yang belum di-review bisa memicu rollback tanpa persetujuanmu. Hanya `deploy.yml` yang boleh memakai label `deploy-<proj>`; agent mengeceknya di setiap review perubahan `.github/`.
   - `deploy.sh` tidak lagi memperbarui dirinya sendiri dari repo (kalau iya, siapa pun yang bisa merge ke main jadi root di server). Sama untuk `docker/docker-compose.yml` (compose menentukan hak container, jadi sama kuatnya dengan root). Kalau sebuah deploy membawa perubahan salah satunya, deploy berhenti dan mencetak perintah upload ulang per file; perintah itu juga ada di kartu DEPLOY.
4. **Opsi SSH:**
   - Buat kunci khusus (bukan kunci pribadimu): `ssh-keygen -t ed25519 -f ci-deploy -C ci-deploy`.
   - Di server, edit `PROJECT` di `deploy-ssh-gate.sh`, lalu `install -o root -g root -m 755 deploy-ssh-gate.sh /usr/local/bin/<proj>-deploy-gate` (bukan di `/root`: user deploy tidak bisa masuk ke sana). Di `~<user>/.ssh/authorized_keys` tambahkan baris public key dengan awalan:
     `command="/usr/local/bin/<proj>-deploy-gate",no-port-forwarding,no-agent-forwarding,no-X11-forwarding,no-pty`
     Dengan awalan itu, kunci ini **hanya** bisa menjalankan deploy, tidak bisa membuka shell.
   - Baris sudoers sama seperti opsi runner.
   - GitHub repo -> Settings -> Secrets and variables -> Actions. Isi sendiri: `DEPLOY_SSH_KEY` (isi file `ci-deploy`), `DEPLOY_HOST`, `DEPLOY_USER`, `DEPLOY_KNOWN_HOSTS` (hasil `ssh-keyscan -t ed25519 <host>`). Jangan pernah menempel nilai ini ke chat.
5. GitHub repo -> Settings -> Branches (atau Rules): lindungi `main` dengan aturan **wajib lolos status check `test`** (dari workflow `ci`). Check ini baru bisa dipilih setelah CI pernah jalan sekali. Repo **private di GitHub Free tidak punya fitur ini** (butuh Pro, Team atau Enterprise): tanpa itu, penjaganya adalah `task.mjs` (ship menunggu CI lolos sebelum merge).

## 4. Sehari-hari
- Minta perubahan seperti biasa. Perubahan biasa langsung di-merge setelah CI lulus.
- Mau dipasang ke server: bilang "deploy". Agent menjalankan `task.mjs deploy`, lalu kamu melihat prompt berisi commit dan task yang ikut, dan tinggal Approve.
- Ada masalah setelah deploy: bilang "rollback". Prompt muncul lagi. Catatan: migration database **tidak** dibatalkan.
- Deploy manual tetap bisa seperti dulu: `/root/<proj>/deploy.sh` di server.
- Deploy pertama lama (cache CI masih kosong, build pertama di server; glyph: 11 menit tes + 6 menit build Rust). Build dibatasi jumlah core-nya, dan sebaiknya deploy di jam sepi yang kamu tentukan.
- Proyek binary (Rust/Go): tes dan build image sudah jalan saat merge ke main (job `prebuild` di runner server). Waktu kamu bilang "deploy", GitHub memakai hasil tes itu dan server tinggal menukar versi: sekitar 20 detik. Mau tes ulang dulu: minta "deploy dengan retest" (`task.mjs deploy --retest`).

## 5. Apa yang terjadi di server (deploy.sh v2)
| Perubahan | Hasil |
|---|---|
| kode PHP/Blade/route | live tanpa restart |
| tampilan (resources/, CSS/JS) | build di salinan, lalu ditukar sekaligus; tidak ada halaman error selama build |
| paket composer | dipasang di folder baru, lalu ditukar |
| nilai `.env` | refresh config, tanpa restart |
| Dockerfile/entrypoint | rebuild image, restart singkat (jarang) |
Kalau health check `/up` gagal, deploy ditandai gagal dan GitHub menampilkannya merah. Lalu putuskan: perbaiki, atau rollback.

Proyek binary (Rust/Go), blue-green: ada dua slot, biru dan hijau; di antara deploy hanya satu yang jalan.
| Langkah | Yang dilihat user |
|---|---|
| versi baru dinyalakan di slot yang kosong, dicek sehat (+ `SMOKE_PATH`) | tidak ada; gagal di sini = deploy ditolak, versi lama tetap melayani |
| proxy (Caddy) dialihkan ke slot baru dengan reload halus | tidak ada; request yang sedang jalan selesai di versi lama |
| slot lama dihentikan (SIGTERM, request terakhirnya diselesaikan) | tidak ada |
| rollback = image sebelumnya di slot kosong, tanpa build | tidak ada, beberapa detik |
Yang masih restart: ganti `APP_BIND`/`APP_PORT` atau image Caddy me-restart proxy (beberapa detik). Di tes: 0 request gagal dari 2801.

## 6. Kalau ada masalah
- Workflow merah: buka tab Actions, klik run-nya, lihat langkah yang gagal. Atau minta agent: "cek deploy terakhir" (`gh run view <id> --log-failed`).
- Runner offline: di server jalankan `sudo ./svc.sh status`, lalu `start`.
- Katalog error lengkap ada di `PLAYBOOK.md` (E1-E35).
