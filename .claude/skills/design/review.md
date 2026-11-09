# UI review checklist (Web Interface Guidelines)

Vendored from vercel-labs/web-interface-guidelines (so no network fetch per review). Review the named files against
these rules; output grouped by file, `file:line - issue` (clickable), terse, no preamble; a clean file gets `✓ pass`.

## Accessibility
- Icon-only buttons need `aria-label`; form controls need `<label>` or `aria-label`.
- Interactive elements need keyboard handlers (`onKeyDown`/`onKeyUp`); `<button>` for actions, `<a>`/`<Link>` for navigation (not `<div onClick>`).
- Images need `alt` (or `alt=""` if decorative); decorative icons `aria-hidden="true"`.
- Async updates (toasts, validation) need `aria-live="polite"`. Use semantic HTML before ARIA.
- Headings hierarchical `<h1>`–`<h6>`; skip link to main; `scroll-margin-top` on heading anchors.
- Meaningful media needs captions/transcripts; media controls need keyboard support.

## Focus states
- Visible focus on interactive elements (`focus-visible:ring-*`); never `outline:none` without a replacement.
- Prefer `:focus-visible` over `:focus`; `:focus-within` for compound controls; sticky headers/overlays must not cover the focused element.

## Forms
- Inputs need `autocomplete` and a meaningful `name`; correct `type`/`inputmode` (email, tel, url, number).
- Never block paste. Labels clickable (`htmlFor` or wrapping). Disable spellcheck on emails/codes/usernames.
- Submit stays enabled until the request starts, spinner during; errors inline, focus first error on submit.
- Placeholders end with `…` and show an example. Warn before navigation with unsaved changes.

## Animation
- Honor `prefers-reduced-motion`. Animate only `transform`/`opacity`; never `transition: all` (list properties).
- Correct `transform-origin`; SVG transforms on a `<g>` wrapper (`transform-box: fill-box`). Animations interruptible.
- Autoplay >5 s needs pause/stop/hide; muted decorative loops stop under reduced motion.

## Typography
- `…` not `...`; curly quotes `“ ”` not `"`; non-breaking spaces (`10&nbsp;MB`, `⌘&nbsp;K`, brand names).
- Loading states end with `…`; `font-variant-numeric: tabular-nums` for number columns; `text-wrap: balance`/`text-pretty` on headings.

## Content handling
- Long content: `truncate`/`line-clamp-*`/`break-words`; flex children need `min-w-0`. Handle empty states. Anticipate short/average/very long user content.

## Images & performance
- `<img>` needs explicit `width`/`height` (CLS); below-fold `loading="lazy"`, above-fold `fetchpriority="high"`.
- Lists >50 items: virtualize. No layout reads in render (`getBoundingClientRect`, `offsetHeight`). Prefer uncontrolled inputs.
- `preconnect` for CDN domains; preload critical fonts (`font-display: swap`); prefer muted looping `<video>` over GIF; short non-essential loops: a Safari H.264 MP4 `<picture>` source, a `prefers-reduced-motion` media condition and a still fallback.

## Navigation, touch, layout, theming, i18n
- URL reflects state (filters, tabs, pagination); links are `<a>`/`<Link>`; destructive actions need confirm or undo.
- `touch-action: manipulation`; `overscroll-behavior: contain` in modals/drawers; page rubber-band off with `overscroll-behavior: none` on `<html>` inside `@media (pointer: fine)` (keep coarse-pointer defaults for pull-to-refresh); disable text selection during drag; gestures need tap/keyboard alternatives.
- Full-bleed needs `env(safe-area-inset-*)`; avoid stray scrollbars; flex/grid over JS measurement.
- `color-scheme: dark` on `<html>` for dark themes; `<meta name="theme-color">` matches background; native `<select>` sets explicit colors.
- Dates/numbers via `Intl.DateTimeFormat`/`Intl.NumberFormat`; detect language via `Accept-Language`; `translate="no"` on brand names and code tokens.
- Controlled inputs with `value` need `onChange`; guard date/time hydration.

## Copy
- Active voice; Title Case for headings/buttons; numerals for counts; specific labels ("Save API Key" not "Continue"); error messages include the fix; second person.

## Anti-patterns to flag
`user-scalable=no`/`maximum-scale=1`; `onPaste`+`preventDefault`; `transition: all`; `outline-none` without replacement; `<div>`/`<span>` click handlers; images without dimensions; large `.map()` without virtualization; inputs without labels; icon buttons without `aria-label`; hardcoded date/number formats; `autoFocus` without justification; animated GIF where video fits; gesture-only actions.
