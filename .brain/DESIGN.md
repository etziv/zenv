# DESIGN (adopted from the current UI 2026-11-09: tokens in src/index.css; max 120 lines, edited in place)

- The native UI (M20) reproduces this look and every screen's layout exactly (D-022); the React UI it was matched to was removed in task 18 (git history before 2026-11-23).
- Style: "front panel": a black header bar with navigation and live port LEDs over a quiet dot-grid work surface; neutral
  black/grey surfaces, green only as an accent; the bright LED green only for things that are live (running service, active tab).
  The boss wants black + green, no green-tinted backgrounds, no logo mark (wordmark "zenv" only). Source: `src/index.css` header comment, the boss 2026-10-19.
- Palette (light / dark, role; exact values in `src/index.css`, mirrored in `src-native/src/theme.rs`):
  - background `#f4f5f5` / `#08090a`; foreground `#111314` / `#e8eaea`; card `#ffffff` / `#15181a`; popover `#ffffff` / `#1a1d1f`
  - primary `#0b8a65` / `#3ddc97` (on-primary `#f4fffa` / `#04140d`); destructive `#d23f31` / `#f2685b`; success `#12935f` / `#4fd99a`; warning `#b7791f` / `#e6ad4c`
  - secondary `#eceeee` / `#1d2022`; muted `#eeefef` / `#1b1e20`; muted foreground `#60666a` / `#9399a0`; accent `#e8eaea` / `#23272a`
  - border `#dfe2e3` / `#2b2f32`; input `#d3d7d9` / `#363b3e`; grid dot 7% / 5% of foreground
  - panel (header, rail, tray): `#0a0b0b` / `#050606`, raised `#161818` / `#141617`, line `#232626` / `#212426`, text `#d9dddc`, muted `#8a908e`
  - LED `#3ee8a0` (live), LED fail `#ff5a4a`; trace under the rail: LED 45% fading to both edges
  - Theme: system, light or dark, chosen in the header or Settings.
- Fonts (bundled via fontsource; native: static TTFs in `src-native/assets/fonts`): display Chakra Petch 600; body IBM Plex Sans 400/500/600;
  mono JetBrains Mono 400/700.
- Spacing / radius / shadows: radius 0.375rem (sm -2px, lg +2px, xl +6px); flat panels, borders instead of shadows; dark cards must contrast with the background.
- Components: shadcn/ui on radix (Button, Switch, Input, DropdownMenu, Dialog, ...), lucide icons; service tiles with ON/OFF/ERR pills
  inside the toggle; toasts top-right under the header; Studio file icons from Material Icon Theme (D-014).
- Signature element: the header LED rail of live ports and the LED dots on running services and tabs.
- Screens (states: each list has an empty state, a spinner while loading, an inline or toast error):
  - Dashboard: every service as an on/off tile, installed languages, Start apps / Stop all.
  - Apps: Herd-style grid with thumbnails + right detail panel (HTTPS, Tailscale, Paths, program, Docker).
  - Languages / Web servers / Databases / Cache: one layout (component sidebar with real logos | servers + installed versions).
  - Tools: AI routers, Codex. Logs: Projects and Services tabs. Settings: HTTPS (CA), theme, paths.
  - Studio window: Files | Editor | Claude (chat or terminal tabs) | Preview; resizable, empty editor closable (D-010).
  - Tray panel: compact (~300 px), one line per service, no shadow frame.
