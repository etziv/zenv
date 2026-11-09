---
name: platform-android
description: "Android (native) defaults: stack candidates, layer mapping, verify commands, scaffold notes, emulator testing, Android security and QA checklists. Load when ARCHITECTURE.md says Platform: android or when proposing an Android stack."
---

# Profile: Android (native)

Used when ARCHITECTURE.md says `Platform: android`. In this kit dev plays every role below (architect, backend, frontend, devops; the `designer` agent proposes the look and dev writes DESIGN.md from the chosen direction); qa and security are the review agents. Everything here is a default or a candidate; dev verifies versions, the current Google Play target-API requirements and recommended libraries from official docs, and decides.

## Role mapping
- architect: module structure (single vs multi-module), app architecture pattern, min/target SDK, dependency choices.
- backend = data and domain layers: repositories, local database, network client, background work, services, dependency injection. If the app also has a server API, the web profile applies to that part.
- frontend = UI layer: Compose or XML screens, navigation, ViewModels/state holders, theming, accessibility.
- designer: Material design, touch targets of at least 48dp, adaptive layouts (phones, tablets, foldables), light/dark, dynamic color, motion; DESIGN.md in terms of the app theme (color scheme, typography, shapes).
- qa, security, devops: see below.

## Common stack candidates (the architect decides; verify versions)
Kotlin; Jetpack Compose with Material 3; ViewModel with StateFlow; Coroutines and Flow; Hilt or Koin if DI is needed; Room; Retrofit/OkHttp or Ktor; DataStore; WorkManager; Navigation for Compose; Media3 for media playback; Coil for images; Gradle with Kotlin DSL and a version catalog.

## Verification commands (examples; the architect writes the real ones)
`./gradlew assembleDebug`, `./gradlew testDebugUnitTest`, `./gradlew lintDebug` (use `gradlew.bat` on Windows), plus detekt or ktlint if chosen. Instrumented tests (`./gradlew connectedDebugAndroidTest`) run on the emulator (`device-test`); a real device is optional.

## Scaffold notes
- Builders create the project files themselves from SCAFFOLD.md (settings.gradle.kts, version catalog, Gradle wrapper, app module); the Android Studio wizard is not needed.
- SDK packages (platforms, build-tools, NDK, CMake), licenses (`sdkmanager --licenses`) and emulators (`avdmanager`) are installed by a builder with the command line tools; each install asks the boss once. JDK, Gradle sync and version checks are done by agents.
- Manual steps for the boss, only these: install Android Studio itself if missing (it ships the SDK command line tools and a JDK); for an optional real-phone check, plug in the phone and accept the USB-debugging prompt. Every test runs on the emulator by an agent; a real phone never blocks.
- Signing keys, keystores, `keystore.properties` and secret entries in `local.properties` stay out of git and out of logs (the task script refuses to commit them).

## Self-check additions (builders)
Debug build succeeds; unit tests and lint pass with no new lint errors; manifest changes reviewed (permissions, exported components); no hard-coded secrets or API keys; stateful screens survive rotation and process death; long work runs off the main thread.

## QA checklist
Unit tests; Compose UI tests; instrumented tests on the emulator (`adb -s emulator-5554`). Device matrix, run on the emulator with adb (`wm size`/`wm density`, `cmd uimode night`, `settings put system font_scale`, `svc wifi`/`svc data`, `pm revoke`, `am kill` for process death, `cmd appops`/`dumpsys deviceidle` for background limits): min and target API levels, small and large screens, rotation, dark mode, font scale, offline and slow network, permission denied flows, background restrictions and battery optimization, back stack and navigation, interruptions (calls, notifications), process death and restore. Performance: startup time, jank, ANR risk, memory leaks, wake locks, large lists. Release build: run the minified (R8) release variant at least once, not only debug.

## Security checklist
Manifest: exported components and intent filters, minimal permissions, `allowBackup`, `debuggable` off in release. Network: no cleartext traffic, a network security configuration, pinning only if justified. Storage: no secrets in plain preferences or files, use the Keystore, scoped storage. WebView: JavaScript and interfaces, file access. Deep links validated. Logs without personal data. Third-party SDKs and dependency vulnerabilities. R8 obfuscation basics. Play policy basics: permissions justified, data safety declarations.

## Release notes (devops, later)
16 KB page size: Google Play requires apps targeting API 35+ to support 16 KB pages (updates that do not are blocked from 2027-02-01): AGP 8.5.1+ (zip-aligned uncompressed .so), NDK r28+ for own native code, third-party .so checked (`zipalign -c -P 16 -v 4 <apk>`), one run on a 16 KB emulator image (`device-test`). App Bundle (AAB), signing configured through CI secrets, versionCode/versionName rules, R8 mapping files stored, Play tracks (internal, closed, production), a Gradle workflow in GitHub Actions. Keystores and passwords stay out of git and out of logs.

## Defaults to avoid over-engineering
Single module until size justifies more. Add DI, multi-module and extra layers only when the brief needs them. Prefer the platform's recommended libraries over many third-party ones.
