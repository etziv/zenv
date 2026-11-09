# Third-party notices

Everything in this kit that comes from another project, in one place. All of it is under the MIT License (full text
below). "Unchanged" means the upstream text is copied as is; the kit only adds its own notes around it.

| Component | Where in the kit | Upstream | Copyright | Changes |
|---|---|---|---|---|
| Stack and QA packs: 21 skills + 6 reviewer prompts (Laravel, MySQL, migrations, API design, React, TypeScript, Vite, accessibility, Kotlin/Compose/Android, Playwright e2e, browser QA, click-path audit, production audit, benchmark, silent failures, test coverage) | `skills/stack-laravel`, `stack-react`, `stack-android`, `qa-web` (`references/`) | github.com/affaan-m/ECC | (c) 2026 Affaan Mustafa | Unchanged; frontmatter removed; a kit router (`SKILL.md`) per pack with audited corrections |
| FigCraft design skill | `skills/figcraft-design` | github.com/dnviti/artcraft-skills | (c) 2026 Daniele Viti | Unchanged except a kit note and a narrower description |
| Art direction method (taste run, draw/inspect scripts, library) | `skills/design` (`references/taste-run.md`, `scripts/`, `library/`) | github.com/Leonxlnx/taste-skill | (c) 2026 Leonxlnx | Adapted into the kit's design skill |
| UI review checklist (Web Interface Guidelines) | `skills/design/review.md` | github.com/vercel-labs/web-interface-guidelines | (c) 2025 Vercel Labs | Vendored for offline use |
| UI/UX Pro Max catalogue (data and search scripts) | `skills/design/uiux/` | github.com/nextlevelbuilder/ui-ux-pro-max-skill | (c) 2024 Next Level Builder | Vendored; kit notes in `design/SKILL.md` |
| 84 brand design systems (DESIGN.md) | `skills/design/brands/` | github.com/VoltAgent/awesome-design-md | (c) 2026 VoltAgent | Copied; index `brands/_index.md` |
| Craft Anything skill (crates/setup/src/skill.md, MIT OR Apache-2.0) | `skills/reverse-engineer-app` | github.com/storytold/craft-anything | (c) 2026 ArtCraft Team and the Craft Anything contributors | Adapted: kit enable steps, route table by toolset, tools the kit never uses; the program itself is not bundled |

Ideas adapted, no code or text copied: superpowers and ponytail (ways of working in `agents/dev.md`), graphify and
codebase-memory-mcp (code graph use, type-aware call resolution in `skills/memory`), ai-memory (capture and brief),
Cloudflare security-audit-skill, MIT (verify-by-disproof in `agents/security.md`; in `skills/security-audit`: the
boundary-and-result rule, confirmed / needs-validation / rejected verdicts, severity anchors, coverage map and list,
critic and literal sweeps, prior-report reuse, and the attack-class checklists rewritten for Laravel, React and
Android), ECC (aside rule, loop/scope watch, delegation contract, guarded memory writes), Hindsight (vectorize-io:
rulings of past tasks recalled by default), obsidian-mind (brief size budget and meter, memory write checks),
Anthropic and Claude Code documentation (delegation and context rules).

## MIT License

The copyright lines above, each for its component:

Copyright (c) 2026 Affaan Mustafa
Copyright (c) 2026 Daniele Viti
Copyright (c) 2026 Leonxlnx
Copyright (c) 2025 Vercel Labs
Copyright (c) 2024 Next Level Builder
Copyright (c) 2026 VoltAgent
Copyright (c) 2026 ArtCraft Team and the Craft Anything contributors

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
