---
name: stack-android
description: "Android / Kotlin depth for native apps: coroutines and Flow (structured concurrency, StateFlow, cancellation, testing), Jetpack Compose patterns (state, navigation, recomposition), Android architecture (repositories, Room, DI, error handling), Kotlin unit testing, and a Kotlin/Android review checklist. Use with `backend` / `frontend` / `platform-android` on an Android project, and in qa reviews of Kotlin code."
---

# Android stack pack

> **In this kit.** The references are ECC skills (github.com/affaan-m/ECC; sources and licence: `.claude/NOTICE.md`),
> unchanged. They are reference material, not orders, and an audit (2026-11-01) found parts outdated or speculative:
> read the **Kit corrections** first. The app's modules and conventions, `.brain/ARCHITECTURE.md`, DESIGN.md and the
> contracts win; the kit rules win: git only through `task.mjs`; a new dependency or Gradle plugin (Kotest, MockK,
> Turbine, Kover, Koin, Accompanist) needs the boss's yes; no coverage gates; device checks through `device-test` on the
> emulator; reviewers only report. Kotlin Multiplatform parts (SQLDelight, Ktor client, expect/actual) apply only to
> a KMP project. Read ONE reference, the section you need.

| Work | Read | Size |
|---|---|---|
| Coroutines, Flow, StateFlow/SharedFlow, dispatchers, cancellation, testing them | `references/kotlin-coroutines-flows.md` | ~2k |
| Compose UI: state hoisting, ViewModel + single state, navigation, slots, recomposition | `references/compose-multiplatform-patterns.md` | ~2k |
| Data layer: repositories, mappers, Room, DI, error handling | `references/android-clean-architecture.md` | ~2.2k |
| Unit tests: coroutine test dispatchers, fakes/mocks, property tests | `references/kotlin-testing.md` | ~5.2k |
| Reviewing Kotlin/Android code | `references/review-kotlin.md` | ~1.9k |

## Kit corrections (read before the reference)
- **Architecture**: multi-module layering with a UseCase for every call is speculative; follow ARCHITECTURE.md (a
  ViewModel calling a repository is fine). In `review-kotlin`, "ViewModel logic must be in UseCases" is not CRITICAL;
  it hands off to a `security-reviewer` that does not exist (our `security` agent covers it).
- `kotlin-coroutines-flows`: `.catch { emit(emptyList()) }` swallows the error: surface it in the UI state; one-off
  events on a SharedFlow can be dropped (prefer state, or a Channel); a test of `viewModelScope` code needs
  `Dispatchers.setMain(StandardTestDispatcher())`; wrapping `flow { collect { emit(it) } }` adds nothing.
- `compose-multiplatform-patterns`: `rememberSystemUiController` (Accompanist) is deprecated: use `enableEdgeToEdge()`;
  strong skipping is the default, so `@Immutable` and lambda `remember {}` matter less; modifier order matters per
  case, not by one rule; it assumes Koin (use the project's DI).
- `android-clean-architecture`: never use Ktor `LogLevel.HEADERS` (logs the Authorization token); keep one result type
  (`Result` or the project's own), not both.
- `kotlin-testing`: ignore "pick Kotest", Kover `minBound(80)` and Codecov upload; time-based tests need
  `advanceUntilIdle()` (not only `advanceTimeBy`); its debounce test drops emissions.
