# Noirlynx: jaringan privat untuk app lokal

> Nama sistem: **Noirlynx** (keputusan bos, 2026-11-10); perintah client-nya `noirlynx`. Dokumen rencana (2026-11-09), belum BRIEF resmi: kolom "Saran" di bawah masih bisa dicoret atau ditambah.

## 1. Tujuan dan fungsi app

**Masalah.** App yang dikembangkan atau dijalankan di PC lokal (Laravel, Go, Vaultwarden, dan lain-lain) sering perlu dibuka dari
device lain: laptop rekan kerja, HP, server kantor. Membuka port ke internet itu berbahaya; memakai layanan pihak ketiga (Tailscale,
ngrok, Cloudflare Tunnel) berarti jaringan dan data bergantung pada perusahaan lain, nama domainnya bukan milik sendiri, dan aturan
aksesnya diatur di luar instansi.

**Solusi.** Sistem jaringan privat buatan sendiri, cara kerjanya seperti Tailscale:
- **Hanya device yang memasang client** yang bisa masuk jaringan dan membuka app. Device lain tidak bisa melihat app sama sekali.
- **Server kontrol dan console web di server sendiri** (Linux), dengan **domain sendiri**. Console web adalah satu-satunya bagian yang
  publik, dan itu pun dilindungi login.
- **Client hanya menghubungkan device ke jaringan privat**, seperti app Tailscale. Client tidak punya menu untuk membuka app.
- **Membuka app lokal** ke jaringan (nama dan HTTPS sendiri, misalnya `https://vaultwarden.<domain>`) dilakukan oleh **program
  pengelola app lokal** di device itu, yang memanggil antarmuka teknis client (padanan cara program memakai `tailscale serve`).

**Fungsi utama.**
1. Menghubungkan device milik user terdaftar ke satu jaringan privat terenkripsi, dari mana pun mereka tersambung (WiFi rumah, kantor,
   data seluler).
2. Membuka app lokal ke jaringan itu dengan nama dan sertifikat HTTPS yang valid.
3. Mengatur siapa boleh membuka app apa (per user, per kelompok, per app).
4. Mengelola semuanya dari satu console web: device, user, akses, app, dan catatan aktivitas.

**Bukan untuk.** Membuka app ke internet publik tanpa client (itu fungsi Funnel di Tailscale, sengaja tidak diambil). Bukan VPN untuk
browsing internet (exit node baru dipertimbangkan nanti).

**Siapa yang memakai.**
- **Admin** (kamu): mengelola jaringan lewat console web.
- **Pemilik app** (developer yang menjalankan app di PC-nya): membuka app lokalnya ke jaringan lewat program pengelola app lokal.
- **Pengguna** (rekan kerja, pegawai): memasang client, login, lalu membuka app yang diizinkan.

## 2. Komponen sistem

```
                        SERVER (Linux, domain sendiri)
              ┌──────────────────────────────────────────────┐
  browser ──► │ Console web (publik, login + 2FA)            │
              │ Server kontrol: user, device, kunci, aturan, │
              │   nama, sertifikat, audit log                │
              │ Relay: meneruskan lalu lintas terenkripsi    │
              └───────────▲───────────────────▲──────────────┘
                          │ WireGuard          │ WireGuard
                 ┌────────┴────────┐  ┌────────┴────────┐
                 │ Client          │  │ Client          │
                 │ (PC pemilik app)│  │ (laptop/HP/     │
                 │  ▲ antarmuka    │  │  server)        │
                 │  │ teknis       │  └─────────────────┘
                 │ Pengelola app   │
                 │ lokal ─► app    │
                 └─────────────────┘
```

| Komponen | Tugas |
|---|---|
| **Server kontrol** | Menyimpan user, device, kunci publik WireGuard, aturan akses, nama, dan app yang dibuka. Membagikan "peta jaringan" ke setiap client: device mana yang boleh dihubungi dan lewat mana. Server **tidak** membaca isi lalu lintas. |
| **Console web** | Halaman admin: Machines, Users, Access, DNS, Services, Logs, Settings. |
| **Relay** | Meneruskan paket WireGuard antar device yang belum bisa tersambung langsung. Paketnya tetap terenkripsi dari ujung ke ujung. |
| **Client** | Dipasang di device. Login, membuat kunci WireGuard sendiri (kunci rahasia tidak pernah keluar dari device), menerima peta jaringan, membuat interface jaringan virtual, dan menjawab nama DNS di dalam jaringan. Tugasnya hanya **menghubungkan device ke jaringan**: tampilannya (ikon di tray) berisi login, status, dan device lain; tidak ada menu membuka app. Client menyediakan **antarmuka teknis** (CLI stabil dengan output JSON, padanan `tailscale status --json` dan `tailscale serve`) untuk program lain di device itu. |
| **Pengelola app lokal** | Program terpisah di PC pemilik app (bukan bagian dari sistem ini). Ia yang punya tombol "buka app ke jaringan", lalu memanggil antarmuka teknis client untuk mendaftarkan nama app dan meneruskan lalu lintas ke port app-nya. Hanya client yang memegang identitas dan kunci device, jadi pendaftaran app selalu lewat client. |

**Teknologi:** lihat bagian 10. Enkripsi dan tunnel memakai **WireGuard** (protokol terbuka, sudah teruji, dipakai juga oleh
Tailscale); protokol enkripsi tidak dibuat sendiri.

## 3. Cara kerja

1. **Daftar device.** User memasang client lalu login (lewat console). Client membuat pasangan kunci WireGuard; kunci publiknya dikirim
   ke server kontrol.
2. **Persetujuan.** Kalau device approval aktif, admin menyetujui device baru di console. Setelah itu device dapat IP privat tetap dan nama.
3. **Peta jaringan.** Server kontrol mengirim ke setiap client daftar device yang boleh dihubunginya (sesuai aturan akses), kunci
   publiknya, dan jalurnya.
4. **Koneksi.** Tahap 1: semua lalu lintas lewat relay di server (sederhana, selalu jalan). Tahap 2: device mencoba tersambung langsung
   (lebih cepat), relay jadi cadangan.
5. **Membuka app.** Di program pengelola app lokal, pemilik app menekan "buka ke jaringan" dan memberi nama, misalnya `vaultwarden`.
   Program itu memanggil antarmuka teknis client (misalnya `noirlynx serve --service vaultwarden 127.0.0.1:9001`). Server kontrol mencatat `vaultwarden.<domain>` → app itu, membuatkan sertifikat HTTPS, dan memberi tahu client yang diizinkan.
6. **Akses.** Pengguna yang diizinkan membuka `https://vaultwarden.<domain>` di browser. Nama itu hanya dikenal di dalam jaringan
   privat; dari luar jaringan, nama itu tidak menuju ke mana pun.
7. **Pencatatan.** Setiap perubahan di console (siapa menambah device, mengubah aturan, membuka app) masuk audit log.

## 4. Fitur Tailscale dan rencana adopsinya

Saran: **MVP** = versi pertama, **Nanti** = tahap berikutnya, **Lewati** = tidak diambil.

### A. Inti jaringan
| Fitur Tailscale | Fungsinya | Saran |
|---|---|---|
| Tailnet + IP tetap | Setiap device dapat IP privat yang tidak berubah (Tailscale: 100.x.y.z), di jaringan mana pun device itu tersambung. | MVP |
| Enkripsi WireGuard | Semua lalu lintas antar device dienkripsi ujung ke ujung. Server pusat hanya membagikan kunci publik, tidak bisa membaca isi. | MVP |
| Koneksi langsung (NAT traversal) | Dua device di belakang router berbeda tetap tersambung langsung tanpa lewat server: lebih cepat, server lebih ringan. | Nanti (tahap 2) |
| Relay (DERP) | Jalur cadangan lewat server saat koneksi langsung gagal; tetap terenkripsi. Bisa di-host sendiri. | MVP (tahap 1: semua lewat relay) |
| Peer relay | Device milik sendiri dipakai sebagai relay untuk device lain. | Lewati |

### B. Nama, DNS, HTTPS
| Fitur Tailscale | Fungsinya | Saran |
|---|---|---|
| MagicDNS | Device dipanggil dengan nama (`pc-kantor`), bukan IP. | MVP |
| Nama tailnet | Akhiran nama semua device (Tailscale: `caracal-alnair.ts.net`). Di sistem ini: domain sendiri. | MVP |
| Nameserver, split DNS, search domain | Mengatur DNS device: domain tertentu diarahkan ke DNS tertentu (misalnya DNS internal kantor). | Nanti |
| Sertifikat HTTPS | Nama di jaringan dapat sertifikat asli, browser tidak memberi peringatan. | MVP |

### C. Membuka layanan
| Fitur Tailscale | Fungsinya | Saran |
|---|---|---|
| Serve | Membuka port lokal (app di PC) ke jaringan, dengan HTTPS. | MVP (antarmuka teknis di client; tombolnya di program pengelola app lokal) |
| Services (`svc:`) | Layanan punya nama dan IP sendiri, terpisah dari device; bisa dilayani beberapa device sebagai cadangan. | MVP (nama per app); cadangan multi-device: Nanti |
| Funnel | Membuka app ke internet publik tanpa client. | Lewati (bertentangan dengan tujuan) |
| Subnet router | Satu device membuka akses ke LAN di belakangnya (printer, server kantor yang tidak bisa dipasang client). | Nanti |
| Exit node | Semua internet device dilewatkan ke device lain, seperti VPN biasa. | Nanti |
| App connector | Akses ke situs SaaS tertentu dilewatkan lewat satu device, supaya IP keluarnya tetap. | Lewati |

### D. Akses dan keamanan
| Fitur Tailscale | Fungsinya | Saran |
|---|---|---|
| Policy (ACL / grants) | Aturan siapa boleh ke mana dan port berapa ("tim keuangan hanya boleh membuka app keuangan"). | MVP (aturan sederhana per user/kelompok/app) |
| Groups | Mengelompokkan user, aturan cukup ditulis sekali. | MVP |
| Tags | Device diberi identitas peran (`server`), bukan pemiliknya. | Nanti |
| Device approval | Device baru harus disetujui admin sebelum masuk jaringan. | MVP |
| Key expiry | Device wajib login ulang berkala (Tailscale: default 180 hari). | MVP |
| Device posture | Akses hanya untuk device yang memenuhi syarat (OS terbaru, antivirus aktif). | Nanti |
| Tailnet lock | Device baru harus ditandatangani device tepercaya; server yang dibobol pun tidak bisa menyusupkan device. | Nanti |
| Tailscale SSH | SSH ke server dengan identitas jaringan, tanpa kunci SSH; bisa minta login ulang dan merekam sesi. | Nanti |
| Ephemeral node | Device sementara (CI, container) otomatis dihapus saat offline. | Lewati |

### E. User dan identitas
| Fitur Tailscale | Fungsinya | Saran |
|---|---|---|
| Login lewat penyedia identitas | Masuk dengan Google, Microsoft, GitHub, atau SSO kantor (OIDC); Tailscale tidak menyimpan password. | MVP: login sendiri (email + password + 2FA); SSO: Nanti |
| User, role, undangan | Mengundang user, memberi peran: owner, admin, anggota, auditor. | MVP |
| Share device | Membagikan satu device ke orang di jaringan lain. | Lewati |
| Auth key | Kunci untuk mendaftarkan device tanpa login manual (server tanpa layar). | MVP |
| API dan OAuth client | Mengelola jaringan lewat program atau skrip. | Nanti |

### F. Fitur di device
| Fitur Tailscale | Fungsinya | Saran |
|---|---|---|
| Client multi-OS | App untuk Windows, macOS, Linux, Android, iOS, Docker, dan lain-lain. | MVP: Windows + Linux; lainnya: Nanti |
| Taildrop | Kirim file antar device. | Nanti |
| Tailscale Drive | Berbagi folder antar device. | Lewati |
| Auto-update client | Client memperbarui dirinya sendiri. | Nanti |
| Diagnosa (`status`, `ping`, `netcheck`) | Melihat device yang online, jalur koneksinya (langsung atau relay), dan kualitas jaringan. | MVP |

### G. Admin dan pemantauan
| Fitur Tailscale | Fungsinya | Saran |
|---|---|---|
| Console web | Halaman Machines, Users, Access, DNS, Services, Logs, Settings. | MVP |
| Audit log | Mencatat siapa mengubah apa di console. | MVP (penting untuk sistem instansi) |
| Network flow log | Mencatat koneksi: device mana ke mana, kapan. | Nanti |
| Webhook / log streaming | Mengirim kejadian (device baru, dan lain-lain) ke sistem lain. | Lewati |

## 5. Usulan isi MVP (versi pertama)

1. Server kontrol + relay di server Linux.
2. Console web: login + 2FA, Machines, Users (role, undangan), Access (aturan sederhana), Services (app yang dibuka), audit log.
3. Client Windows dan client Linux.
4. Jaringan WireGuard, semua lalu lintas lewat relay di server.
5. Nama device dan nama app di domain sendiri, dengan sertifikat HTTPS.
6. Nama per app + HTTPS untuk app lokal, dan hanya user yang diizinkan yang bisa membukanya.
7. Antarmuka teknis client (CLI + output JSON: status, daftarkan/cabut app) yang dipakai program pengelola app lokal; client
   sendiri tidak punya menu membuka app.
8. Device approval, key expiry, auth key.
9. Diagnosa di client: online/offline dan jalur koneksi.

## 6. Keamanan

- Isi lalu lintas dienkripsi WireGuard dari device ke device; server kontrol dan relay tidak bisa membacanya.
- Kunci rahasia device dibuat dan disimpan di device itu sendiri, tidak pernah dikirim ke server.
- Console web publik, jadi wajib: login + 2FA, batas percobaan login, sesi yang kedaluwarsa, dan audit log setiap perubahan.
- Default menolak: device baru tidak bisa apa-apa sampai disetujui, dan app hanya bisa dibuka oleh yang diberi izin.
- Device yang hilang atau user yang keluar dicabut dari console dan langsung terputus dari jaringan.

## 7. Domain dan DNS

**Tanpa fun name.** Tailscale memakai nama acak (`<fun-name>.ts.net`) karena jutaan tailnet berbagi satu domain `ts.net`. Sistem
ini tidak memakainya. Nama dibuat fleksibel:
- **Domain jaringan diatur admin di console** (Settings): domain instansi, subdomainnya, atau domain lain milik sendiri, misalnya
  `net.<domain-instansi>`. Bisa diganti kapan saja: client otomatis memakai nama baru, sertifikat dibuat ulang.
- **Nama device dan nama app bebas** (`vaultwarden`, `simpeg`, `pc-hanif`), diatur dari console atau program pengelola app lokal.
- **Nama app boleh di luar domain jaringan**, misalnya langsung `vaultwarden.<domain-instansi>`: client yang menjawab nama itu di
  dalam jaringan. Batasnya hanya sertifikat: nama di bawah subdomain yang didelegasikan ke server mendapat sertifikat otomatis; nama
  di luar itu butuh satu record `_acme-challenge` per nama dari pengelola DNS.
- Jaringan terpisah per unit kerja, bila nanti diperlukan: satu tingkat lagi (`<unit>.net.<domain-instansi>`).

| Bagian | Tailscale | Sistem ini |
|---|---|---|
| Domain jaringan | `<fun-name>.ts.net` (milik Tailscale) | `net.<domain-instansi>` |
| Nama device / app | `pc.<fun-name>.ts.net`, `vaultwarden.<fun-name>.ts.net` | `pc.net.<domain-instansi>`, `vaultwarden.net.<domain-instansi>` |
| Penjawab nama | resolver Tailscale di device (100.100.100.100) | resolver di client |
| Sertifikat HTTPS | diurus Tailscale (DNS `ts.net` miliknya) | diurus server kontrol lewat subdomain itu |
| Console | `login.tailscale.com` | `console.net.<domain-instansi>` |

1. **Nama dijawab di dalam client (split DNS).** Client hanya menangani akhiran `*.net.<domain-instansi>` dan menjawabnya dengan IP
   privat; nama lain tetap ke DNS biasa. Dari luar jaringan, nama app tidak menuju ke mana pun.
2. **Sertifikat HTTPS lewat DNS-01.** Browser hanya percaya sertifikat publik (Let's Encrypt); untuk nama yang tidak terbuka ke
   internet, server kontrol harus bisa menulis record `_acme-challenge` di subdomain itu. Yang diminta ke pengelola DNS instansi
   (Diskominfo), salah satu:
   - **delegasi subdomain** `net.<domain-instansi>` (record NS) ke server sendiri, sekali saja, lalu server mengatur subdomain itu
     sendiri dan memakai **satu sertifikat wildcard** `*.net.<domain-instansi>`;
   - atau satu **CNAME `_acme-challenge.net.<domain-instansi>`** ke server sendiri: tetap bisa wildcard, permintaannya lebih kecil.
3. **Console** cukup satu record A, `console.net.<domain-instansi>` → IP server; hanya itu yang publik.
4. **Wildcard menjaga nama app tetap rahasia.** Setiap sertifikat Let's Encrypt tercatat di log publik (Certificate Transparency):
   dengan sertifikat per nama, setiap nama app ikut tercatat; dengan wildcard hanya `*.net.<domain-instansi>`. Kuota Let's Encrypt
   (sekitar 50 sertifikat baru per minggu per domain terdaftar, dipakai bersama unit kerja lain di domain instansi) juga aman.
5. **Alternatif tanpa DNS publik:** CA internal milik sendiri, dipasang otomatis oleh client ke device. Kekurangannya: Firefox dan
   device tanpa client akan memberi peringatan (device tanpa client memang tidak bisa masuk jaringan).

## 8. Tahapan

| Tahap | Isi |
|---|---|
| 1 (MVP) | Bagian 5 di atas. Semua lalu lintas lewat relay di server. |
| 2 | Koneksi langsung antar device, client Android, Taildrop, tags, split DNS. |
| 3 | SSO (OIDC), device posture, subnet router, exit node, Tailscale SSH versi sendiri, network flow log, API. |

## 9. Yang belum diputuskan

1. Device apa saja yang harus bisa memasang client di MVP (Windows dan Linux saja, atau juga Android)?
2. Jumlah user, device, dan app yang diperkirakan.
3. Seberapa rinci aturan akses di MVP: per user, per kelompok, per app, per port?
4. Nama repo dan folder project-nya (nama sistemnya sudah: Noirlynx).
5. Subdomain yang akan dipakai dan izin dari pengelola domain.

## 10. Teknologi

Keputusan bos (2026-11-09): **seluruh basis kode Rust** (stabil, cepat, multiplatform); console web memakai backend Rust dan frontend
React; client desktop **tanpa webview**, memakai pilihan yang paling optimal untuk project ini.

| Bagian | Teknologi |
|---|---|
| Server kontrol + relay | Rust |
| Console web | Backend Rust (API), frontend React |
| Client desktop | Rust, UI native tanpa webview |
| Tunnel | WireGuard (implementasi Rust atau driver WireGuard bawaan OS, dipilih di proposal arsitektur) |

**Susunan client yang disarankan (paling optimal, sama seperti pola Tailscale):**
1. **Service latar belakang** (daemon / Windows service), satu-satunya bagian yang butuh hak administrator: membuat interface jaringan
   virtual, menjalankan WireGuard, menyimpan kunci device, mengurus DNS dan serve. Jalan terus walau tidak ada yang login ke tampilan.
2. **CLI** yang berbicara dengan service lewat saluran lokal (named pipe di Windows, Unix socket di Linux). Karena service yang
   bekerja, CLI dan program lain (pengelola app lokal) tidak perlu hak administrator. Inilah antarmuka teknis di Lampiran A.8.
3. **Tampilan kecil** (ikon tray + satu jendela: login, status, daftar device), juga lewat saluran lokal yang sama. Tampilan ini
   proses terpisah yang ringan: ditutup pun koneksi jaringan tetap jalan.

**Toolkit UI native (tanpa webview).** Kandidat: **Slint** (deklaratif, ringan, juga mendukung Android untuk tahap berikutnya) dan
**iced** (Rust murni, berlisensi MIT). Pilihan akhir, versinya, dan syarat lisensinya ditetapkan di proposal arsitektur setelah dicek
di dokumentasi resmi terbaru.

## 11. Catatan: optimasi alur (dikerjakan kemudian)

Alur di Lampiran A adalah cara kerja dengan Tailscale; dengan client sendiri, alurnya bisa dibuat lebih efisien. Dicatat untuk nanti,
tidak mengubah isi MVP:
1. **Event, bukan polling.** Pengelola app lokal sekarang memanggil `status --json` tiap 60 detik untuk mendeteksi perubahan nama.
   Client sendiri bisa mengirim kejadian (tersambung, putus, nama berubah, app dibuka atau ditutup) lewat saluran lokal yang terus
   terbuka (`noirlynx status --watch`), jadi perubahan langsung terlihat tanpa memanggil proses berulang-ulang.
2. **API lokal, bukan memanggil CLI per perintah.** Setiap perintah `tailscale` berarti menjalankan proses baru. Program lain bisa
   langsung bicara ke service lewat named pipe / Unix socket dengan protokol JSON, lebih cepat dan hasilnya terstruktur. CLI tetap ada
   untuk manusia dan skrip.
3. **Identitas pengunjung asli, bukan IP pengganti.** Sekarang pengunjung diberi alamat palsu `100.64.0.0` supaya tidak dianggap
   localhost. Client sendiri bisa meneruskan identitas yang sebenarnya (user, device, IP jaringan privat) dalam header yang ditandatangani,
   sehingga app tahu siapa yang membuka dan bisa memakainya untuk login otomatis atau log.
4. **Filter nama di client.** Penolakan nama host yang salah (anti DNS rebinding) bisa dilakukan client sebelum lalu lintas sampai ke
   web server lokal, sehingga konfigurasi web server tidak perlu daftar nama yang harus diperbarui setiap nama berubah.
5. **Satu jalur untuk mode port dan nama sendiri.** Client bisa meneruskan langsung ke port app tanpa port loopback perantara per app,
   kalau web server lokal mendukung routing per nama.
6. **Koneksi langsung antar device** (tahap 2) agar lalu lintas tidak selalu lewat server.
7. **Sertifikat per nama vs wildcard.** Satu sertifikat wildcard untuk semua nama app (lebih sedikit permintaan sertifikat, cepat
   saat app baru dibuka), bila delegasi subdomain tersedia.

## Lampiran A. Acuan teknis: cara pengelola app lokal mem-forward app lewat Tailscale

Ini alur yang sudah berjalan dan terbukti (diuji dengan Apache, Nginx, FrankenPHP, dan Tailscale sungguhan). Client sistem ini harus
menyediakan padanan setiap langkah yang menyentuh Tailscale (Lampiran A.8), supaya pengelola app lokal bisa memakai keduanya dengan cara
yang sama.

### A.1 Mendeteksi Tailscale
1. Cari `tailscale.exe` di PATH, lalu di `%ProgramFiles%\Tailscale\`. Tidak ketemu: "Tailscale belum terpasang".
2. Jalankan `tailscale status --json` (batas waktu 20 detik) dan baca:
   | Field | Dipakai untuk |
   |---|---|
   | `BackendState` = `"Running"` dan `Self.Online` = true | Tailscale tersambung |
   | `Self.DNSName` (titik di ujung dibuang) | nama lengkap, misalnya `desktop-b6kgt13.caracal-alnair.ts.net`; nama pendek = bagian sebelum titik pertama; nama tailnet = sisanya |
   | `Self.TailscaleIPs` | IP device (IPv4 dan IPv6) |
   | `Self.Tags` | device ber-tag atau tidak (syarat Service) |
   | `CertDomains` tidak kosong | sertifikat HTTPS aktif di tailnet (syarat Service) |
3. Pesan masalah yang ditampilkan: belum terpasang, tidak jalan, belum tersambung (perlu login), device belum ber-tag, HTTPS belum aktif.

### A.2 Port lokal per app
- Setiap app yang dibuka ke tailnet mendapat **port loopback sendiri**, mulai 9001: port bebas pertama yang belum dipakai app tailnet lain.
  Port ini disimpan per app dan tetap sama selama app terbuka.
- Web server lokal app itu (Apache, Nginx, FrankenPHP, atau Caddy) mendapat **listener tambahan `127.0.0.1:<port>`** yang hanya
  melayani app itu. Domain lokal app (`app.test`) tetap jalan seperti biasa di port normal.
- Lalu lintas dari tailnet selalu masuk lewat port ini, tidak pernah lewat port web server yang normal.

### A.3 Pengaman di port itu
1. **Filter nama host (anti DNS rebinding).** Port itu hanya menjawab request yang header `Host`-nya adalah nama atau IP tailnet PC ini
   (nama pendek, nama lengkap, setiap IP), ditambah nama Service app itu sendiri. Nama lain dijawab 421/403. Tanpa ini, halaman web
   jahat bisa mengarahkan namanya ke port ini dan memakai app lewat browser korban; nama Service app lain juga ditolak.
2. **Pengunjung bukan "localhost".** Lalu lintas dari `tailscale serve` tiba dari `127.0.0.1`, sehingga app mengira pengunjung adalah
   PC ini sendiri (dan bisa membuka halaman "khusus lokal"). Karena itu alamat pengunjung diganti `100.64.0.0` (alamat pertama rentang
   Tailscale, tidak dipakai device mana pun): `REMOTE_ADDR` untuk PHP, `X-Forwarded-For` dan `X-Real-IP` untuk app reverse proxy.
3. Header penanda app (berisi id app) supaya backend bersama (FrankenPHP) tahu app mana yang dituju.

### A.4 Mode port: `http://<pc>:<port>`
```
tailscale serve --bg --tcp=<port> tcp://127.0.0.1:<port>
```
- App bisa dibuka di `http://<nama-pc>:<port>`, `http://<nama-lengkap>:<port>`, dan `http://<ip-tailnet>:<port>` dari device mana pun di tailnet.
- Mode TCP dipilih karena berlaku untuk IP maupun nama (mode HTTP Tailscale hanya mencocokkan nama). Isinya HTTP biasa: lalu lintasnya
  sudah dienkripsi WireGuard. `--bg` membuat pengaturan ini bertahan setelah restart. Tidak perlu hak administrator.

### A.5 Mode nama sendiri: `https://<nama>.<tailnet>.ts.net` (Tailscale Service)
```
tailscale serve --service=svc:<nama> --https=443 http://127.0.0.1:<port>
```
- Tailscale yang memegang HTTPS (sertifikat Let's Encrypt untuk nama itu), lalu meneruskan ke port lokal sebagai HTTP.
- Aturan nama: 1 sampai 63 karakter, huruf kecil, angka, dan tanda hubung, tidak diawali atau diakhiri tanda hubung. Satu nama hanya untuk satu app.
- Syarat di sisi tailnet (diatur di admin console atau API):
  - PC host ber-tag (misalnya `tag:server`), dengan `tagOwners` di policy;
  - HTTPS certificates aktif (halaman DNS);
  - Service `svc:<nama>` didefinisikan, dengan port `tcp:443`;
  - host disetujui: otomatis lewat `autoApprovers.services`, atau manual oleh admin;
  - grant akses, misalnya `{"src": ["autogroup:member"], "dst": ["svc:<nama>"], "ip": ["443"]}`.
- Akibat sampingan: device ber-tag bukan lagi `autogroup:member`, jadi PC host sendiri tidak bisa membuka URL Service-nya; dari PC itu
  pakai domain lokal app.

### A.6 Menutup app
```
tailscale serve clear svc:<nama>          (mode nama sendiri)
tailscale serve --tcp=<port> off          (mode port)
```
Lalu catatan app dihapus dan konfigurasi web server diterapkan ulang tanpa listener tailnet. Kalau perintah Tailscale gagal (entri sudah
tidak ada), penutupan tetap dilanjutkan.

### A.7 Saat nama berubah (PC di-rename atau nama tailnet di-re-roll)
Nama dan IP tailnet terakhir disimpan, supaya konfigurasi web server tidak bergantung pada Tailscale yang sedang jalan. Setiap 60 detik
(dan setiap kali daftar app tailnet dibaca), selama ada app yang terbuka dan Tailscale online, nama sekarang dibandingkan dengan yang
tersimpan; kalau berbeda, nama baru disimpan dan konfigurasi semua web server diterapkan ulang. Tailscale offline tidak menghapus nama tersimpan.

### A.8 Padanan yang harus disediakan client Noirlynx
| Tailscale | Padanan di client (antarmuka teknis) |
|---|---|
| `tailscale status --json` | `noirlynx status --json`: status tersambung, nama pendek dan lengkap, IP, peran/tag device, sertifikat HTTPS tersedia, domain jaringan |
| `tailscale serve --bg --tcp=<port> tcp://127.0.0.1:<port>` | `noirlynx serve --tcp=<port> tcp://127.0.0.1:<port>`: membuka port PC ini ke jaringan |
| `tailscale serve --service=svc:<nama> --https=443 http://127.0.0.1:<port>` | `noirlynx serve --service=<nama> --https=443 http://127.0.0.1:<port>`: nama sendiri + HTTPS, sertifikat diurus sistem |
| `tailscale serve clear svc:<nama>` / `--tcp=<port> off` | `noirlynx serve clear <nama>` / `--tcp=<port> off` |
| Tag, HTTPS, definisi Service, approval, grant (console) | Halaman Services dan Access di console: siapa boleh meng-host nama, siapa boleh membuka |

Syarat perilaku antarmuka itu: tidak butuh hak administrator; pengaturan serve bertahan setelah restart; exit code 0 kalau berhasil dan
pesan error yang jelas kalau gagal; output JSON yang stabil (field tidak berganti nama antar versi).
