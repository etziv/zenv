---
name: frontend
description: "Build screens from DESIGN.md against the contracts: navigation, state, theming, accessibility, proven with screenshots. Use for any screen or UI-layer task; creating or changing the look is `design` and the `designer` agent."
---

# Frontend (build screens)

Visual decisions — art direction, DESIGN.md, matching a brand, the anti-slop house rules, UI review — live in the `design` skill. Load `design` to CREATE or revise the visual system; load this skill to BUILD the screens from an existing DESIGN.md against the contracts.

## DESIGN.md first
1. `.brain/DESIGN.md` complete: follow it, never ask again. A small change inside that look needs no card.
2. The boss picked a direction on the DESIGN card: write DESIGN.md from that `## Direction <n>` section of `.brain/design/proposal/directions.md` (load `design`): palette with hex and roles (light and dark), type, spacing, radius, components, signature element, and EVERY MVP screen with its states (empty, loading, error). The direction's key screens are frames in `.brain/design/proposal/proposal.figcraft` (PNGs in `screens/`): they are the visual target for those screens. The boss edited that file after choosing: read the changed frames with the `figcraft` tools (`open_file`, `get_node`, `screenshot`; skill `figcraft-design` section 10) and update DESIGN.md before building. Generated code from FigCraft is a hint, never the implementation.
3. `.brain/design/` holds a Claude Design bundle (anything outside `proposal/`): it is the source of truth; extract it into DESIGN.md the same way.
4. None of these (a new look or a redesign): the main dev calls the `designer` agent; it uses the boss's references named in BRIEF.md and returns the proposal for the DESIGN card.
5. Every new screen: the `design` skill's "Before ANY drawing" gate (the research pack and the library entries the
   screen names, `library.mjs show`), then DESIGN.md. A small change inside an existing screen needs DESIGN.md only.
Platform notes come from the platform skill (for example Material 3 and 48dp touch targets on Android).

Anti-generic rules: no default Inter, Roboto, Arial, Open Sans or system-ui (no stock Android theme); a distinctive display font plus a different readable body font; no purple-to-blue gradients on white, unchanged component-library look, emoji icons, identical rounded-card grids or stock hero + three cards. One dominant color, one sharp accent, neutrals with intent. If it could belong to any AI-built app, redo it.

## Screens
Framework depth, one reference at a time: React/Vite -> `stack-react`; Jetpack Compose -> `stack-android`.
Build from DESIGN.md against `.brain/contracts/` (screens come after all backend modules, so the contracts are real). A contract that is wrong or missing something: fix the backend side first, never guess around it.
Transitions (enter / exit, dialogs, lists, toasts, navigation) follow DESIGN.md's Motion tokens and `design/references/app-motion.md`: the platform's own tools first (Compose animation APIs, Livewire `wire:transition` / Alpine, React `motion`), reduced motion respected, only `transform`/`opacity` on the web. A component from 21st.dev or a new animation package is an install: ask the boss first.

## Plan first, then prove every screen yourself
Plan at the top of `.brain/handoff/<id>-dev.md` and begin at once. There is no QA per module before the MVP review. So: unit tests for state and logic, the verify commands, and every screen run on the emulator or in the browser (`device-test`) with a screenshot you read yourself: matches DESIGN.md, empty/loading/error states, touch targets, no layout break. Close every screen module with your module review in the handoff: each acceptance criterion with evidence and screenshot, the user flows it touches updated in `.brain/FLOWS.md` (screen -> ViewModel/controller -> API steps, one History line), the conformance items it touches, the full suite green.
