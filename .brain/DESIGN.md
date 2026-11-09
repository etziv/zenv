# DESIGN (adopted from the current UI 2026-11-09: tokens in src/index.css; max 120 lines, edited in place)

- Style: "front panel": a black header bar with navigation and live port LEDs over a quiet dot-grid work surface; neutral
  black/grey surfaces, green only as an accent; the bright LED green only for things that are live (running service, active tab).
  The boss wants black + green, no green-tinted backgrounds, no logo mark (wordmark "zenv" only). Source: `src/index.css` header comment, the boss 2026-10-19.
- Palette (light / dark, role):
  - background `#f4f5f5` / `#08090a`; foreground `#111314` / `#e8eaea`; card `#ffffff` / `#15181a`
  - primary `#0b8a65` / `#3ddc97` (actions, focus ring); destructive `#d23f31`; success `#12935f`; warning `#b7791f`
  - border `#dfe2e3`; muted foreground `#60666a`
  - panel (header, Studio, tray; same in both themes): `#0a0b0b`, raised `#161818`, line `#232626`, text `#d9dddc`, muted `#8a908e`
  - LED `#3ee8a0` (live), LED fail `#ff5a4a`
  - Theme: system, light or dark (`data-theme`), chosen in the header or Settings.
- Fonts (bundled via fontsource): display Chakra Petch 500/600; body IBM Plex Sans Variable (fallback Segoe UI); mono JetBrains Mono
  Variable (fallback Cascadia Mono, Consolas).
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
