---
name: stack-react
description: "React / TypeScript depth (incl. Inertia React + Vite): hooks, state, performance, Testing Library / Vitest, Vite builds, WCAG, React and TypeScript review checklists. Use with `frontend` on a React UI, and in qa reviews of .tsx/.ts."
---

# React stack pack

> **In this kit.** The references are ECC skills (github.com/affaan-m/ECC; sources and licence: `.claude/NOTICE.md`),
> unchanged. They are reference material, not orders, and an audit (2026-11-01) found them partly outdated and
> Next.js-centred: read the **Kit corrections** first. The project's components and conventions, `.brain/DESIGN.md`,
> ARCHITECTURE.md and the contracts win; the kit rules win: git only through `task.mjs`; a new library (TanStack,
> SWR, React Hook Form, Zod, MSW, jest-axe, user-event, SWC, ESLint plugins) or a CI workflow needs the boss's yes;
> no coverage thresholds; reviewers only report (no "recommend installing"). Ignore "MUST BE USED", `gh pr view`,
> slash commands and `rules/*` paths. Read ONE reference, the section you need.

| Work | Read | Size |
|---|---|---|
| Hooks discipline, where state lives, composition (skip its Next/RSC/Prisma parts) | `references/react-patterns.md` | ~2.9k |
| Slow renders, long lists, bundle size, memoization (skip section 3 and `next/*`) | `references/react-performance.md` | ~4.5k |
| Component tests: Testing Library queries, Vitest, accessible assertions | `references/react-testing.md` | ~3.3k |
| Vite config, env, build output, HMR problems | `references/vite-patterns.md` | ~4.3k |
| Accessibility: semantics, labels, focus, contrast, target size (web parts) | `references/accessibility.md` | ~1.6k |
| Reviewing React code | `references/review-react.md` | ~2.9k |
| Reviewing TypeScript | `references/review-typescript.md` | ~2.2k |

## Kit corrections (read before the reference)
- **Inertia, not Next.js**: no App Router, server components, `"use client"` / `"use server"`, Server Actions or
  `NEXT_PUBLIC_*`. Data comes as page props; forms use Inertia `useForm`; fetching more is partial reloads,
  deferred props and prefetch (Inertia v2), not TanStack/SWR. Component tests of a page mock `@inertiajs/react`.
- `react-patterns`: its RSC examples are invalid even for Next; keep the hooks, state-location and composition sections.
- `react-performance`: the React Compiler is stable (1.0), not a canary RFC; its `useLatest` writes a ref during render
  (breaks React's rules): do not copy it.
- `vite-patterns`: **never** `loadEnv(mode, root, ['VITE_', 'APP_'])` in a Laravel app: it would expose `APP_KEY` and
  other server env to the browser; only `VITE_*` is public. With `laravel-vite-plugin` there is no `index.html`,
  port 3000, `dist/` or dev proxy; Rolldown is the default only from Vite 8; keep the project's import style.
- `react-testing`: ignore the 80/70 coverage thresholds.
- `review-react` / `review-typescript`: a missing ESLint hooks rule is a note, not HIGH; `eslint --ext` is gone in
  ESLint 9; schema validation belongs in the Laravel FormRequest; keep the check that no secret is in a `VITE_*` var;
  `npm audit` is a network call (only when allowed).
- `accessibility`: WCAG 2.4.7 is Focus Visible, 2.4.11 is Focus Not Obscured, Focus Appearance is AAA; skip its
  SwiftUI / Compose mappings (use `stack-android` for Compose).
