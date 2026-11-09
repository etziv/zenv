# Memory tool: measurements and model details (moved from SKILL.md word for word)

## Code graph accuracy (old SKILL.md lines 41-64)
graphify alone reads syntax only, so it misses most `obj.fn()` calls (measured: 14-17 % of member calls on two
Kotlin projects). The skill adds them the way codebase-memory-mcp does, without its binary: receiver types from
declarations, constructors, parameters, fields, casts and declared return types, followed through call chains and
property paths (`container.outbox.enqueue()`), supertypes, `this`; library types (`Modifier`, `Flow`) and string
literals, private members of other classes, test code as a target of production code, and standard collection /
coroutine verbs on an unknown receiver are excluded; an unknown receiver links only when exactly one visible
definition fits. Measured head-to-head on nuq and pankon-mobile (file -> called function pairs): graphify 39 % / 51 %,
this skill 74 % / 84 %, CBM 85 % / 89 % but with about a fifth of its call edges wrong in samples (`String.format`,
`File.exists`, `scope.launch` tied to repo functions of the same name, 155 production -> test edges on nuq); CBM's
precise LSP subset alone 39 % / 35 %. This skill's sampled precision: 30/30 on the last round. Still not resolved:
lambda parameters (`it`), generics, extension functions, overloads.

PHP (Laravel/Filament) adds: per-function types of `$locals` (a name reused in two functions keeps each type), helper
return types (`$sk = $this->skPengurangan()`), container `app(X::class)`, query chains ending in one model
(`X::where(..)->first()`), Eloquent calls that return their receiver (`->fresh()`, `->refresh()`), Blade views,
relations read as a property (`$tx->notary?->name`) or named in strings (`with('notary')`, `'notary.name'`), and
`unique`: a method defined once in the repo and nowhere in vendor/ is the target of `$anything->name()` even when the
receiver is untyped (`fn ($record) => ...`). Measured on ebphtbv3 (real calls, comments and strings excluded): 89 % ->
96 % of file -> function pairs, Blade 0 -> 51 %, relation properties 6 -> 66 %, tests 99 %; sampled precision 36/36.

So the graph is a strong lower bound, never the whole truth: `uses` therefore lists EVERY whole-word hit of the name
in the repo, sorted: graph-certain uses (with the caller and how it was resolved), code the graph could not resolve
(same name, check the receiver), strings, and comments (`--all`). It is as complete as `git grep -w` and says which
hits are certain. Use Grep only for a literal string (a message, a config value), not to find a symbol's users.

## The brain's link model (old lines 69-84)
The model keeps nothing between prompts (every turn re-sends the context), so the long-term memory lives here, on
disk, and only small pieces enter the context when needed. Every memory unit (section, D-/G-/F- entry, table row) is a
node, linked by what its text says: a markdown link (1.0), `[superseded by D-x]` (0.9), an id it names (0.8), a code
symbol or repo path it names (0.7, two-part names only: `TaskRejections`, not `status`), the same task (0.5), the same
table (0.4). Units recalled together get a co-recall edge that grows with use (1 - e^(-n/3)) and fades (half-life 30
days); nothing is deleted. Cached in `graphify-out/brain.json`, rebuilt when a memory file or the code graph changes.
(The `brain` command and the hooks as they work now: SKILL.md.)

## `mem` ranking details and measurements (old lines 139-148)
- How `mem` ranks (0 LLM): BM25 per section, heading words double; each question word also matches its stems
  (Indonesian affixes: `pembayaran`/`dibayar` -> `bayar`; English `-ed/-ing/-es`), counted once per word; a word the
  notes rarely use is also looked up in the other language (`refund` -> `restitusi`); "latest / terakhir / sekarang"
  favours recently dated sections and STATUS.md. Measured on ebphtbv3 (41 questions): right note first 54 -> 66 %,
  shown at all 63 -> 80 %, stale note above its replacement 2/3 -> 0/3; English-memory projects unchanged.
