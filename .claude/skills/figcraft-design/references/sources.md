# Sources and attribution

This skill is original writing that adapts ideas and workflows from the sources below. No source
was copied in bulk; short command examples follow FigCraft's own documented API.

| Source | What was used | License |
|---|---|---|
| FigCraft, github.com/storytold/figcraft: `CLAUDE.md`/`AGENTS.md`, `docs/mcp.md` (conventions, tools, prompts, Dev Mode, prototypes, examples), `crates/mcp/src/prompts.rs` (the 4 built-in workflows `mobile_screen`, `design_system`, `landing_page`, `prototype_flow`, adapted in `workflows.md`) | Tool conventions, command semantics, workflow steps | MIT OR Apache-2.0 (repo reports Apache-2.0; `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`) |
| Installed `figcraft` MCP server, `list_commands` output (October 2026) | Command ids and parameter docs in `command-catalog.md`; behaviour notes in `gotchas.md` from a throwaway test session; for the divergent-directions step (SKILL.md §2), verified in a headless throwaway session: `page.add` and `page.duplicate` make the new page current, identical variable names coexist in different collections, `variable.export {collections, format:"css"}` exports one collection, `variable.renameCollection` works | n/a (observed behaviour) |
| Anthropic `frontend-design` skill, github.com/anthropics/skills (`skills/frontend-design/SKILL.md`) | Design principles: grounding in the subject, bold aesthetic direction, plan-then-critique process, typography guidance, spatial composition and motion, list of generic AI-design tells, restraint, quality floor, UX writing guidance (paraphrased in SKILL.md §2–3, `distinctiveness.md`, `design-to-code.md` §11) | Apache-2.0 (`LICENSE.txt` in the skill folder) |
| Figma MCP server guide, github.com/figma/mcp-server-guide: `figma-power/steering/implement-design.md`, `create-design-system-rules.md`, `code-connect-components.md`; `skills/figma-design-to-code`, `figma-generate-design`, `figma-generate-library`, `figma-use/references/gotchas.md` | Workflow structure only: fetch context → screenshot reference → assets → translate to project conventions → 1:1 validation checklist; scope contract for design-system work; reuse-before-create; rules-file idea; Code Connect concept | No license file found in the repository at the time of reading: treated as all rights reserved, so only ideas were used, rewritten in original words, with nothing copied |
| Unpublished `distinctive-ui` draft by the repository author (Italian) | The concept behind "uniqueness is the default": the model converges on the average, and originality comes from improbable inputs (an outside-software metaphor, real non-SaaS references), arbitrary constraints, the user choosing among divergent directions and an outside critique; the brief items, divergent directions, anchor component, signature interaction, ban list, component-library reuse rules and adversarial review. Rewritten in English and folded into this skill (the draft's FigCraft section described a different, Figma-plugin tool and was not used) | MIT (this repository) |
| Björn Ottosson, "A perceptual color space for image processing" (OKLab), bottosson.github.io | Matrices of the OKLCH-to-hex helper in `distinctiveness.md` | public description of the colour space; the helper is original code |
| W3C WCAG 2.x relative luminance and contrast ratio definitions | Contrast helper in `distinctiveness.md` | W3C document licence (definitions only; code is original) |

Not verified against an app or a live service: font foundry licence terms (`distinctiveness.md` asks
to re-check them at download time), third-party component library licences, the Playwright CLI flags
(`design-to-code.md` §11 asks to check `--help`), and concurrent use of one FigCraft MCP session by
several subagents (SKILL.md §2 builds directions sequentially for that reason).

Trademarks: Figma is a trademark of Figma, Inc.; FigCraft and ArtCraft names/logos belong to their
owners. This skill is not affiliated with or endorsed by them.
