# Supply chain, CI and release

Discipline: a loose version or a package with a known CVE alone makes no finding. It needs three facts: who can change
what gets installed or built, which build uses it, and what that build can then do (run code with secrets, publish, deploy). CI
files are authorization code: for each workflow, find the event that triggers it, whose code it runs, which secrets and
token permissions it has, and what it can publish or change. Repository settings (branch protection, environment
reviewers, Play Console) are outside source: needs validation.

## Dependencies
- **A new package.** Is it the intended one (name close to a popular package, few downloads, one new maintainer,
  abandoned)? Does it run code at install (`postinstall`, Composer plugins: check `config.allow-plugins`, Gradle
  plugins)? Is the lockfile committed and changed together with the manifest?
- **Audit, read-only.** `composer audit`, `npm audit` (no `fix`); for Gradle, compare versions with the advisory
  databases (OSV, GitHub advisories). A vulnerable version is a finding only when the vulnerable code is reachable
  from our input; otherwise list it under Hardening with the upgrade.
- **Where packages come from.** `composer.json` `repositories` (vcs, path, a private Satis) take priority over
  Packagist; `.npmrc` registries and scopes (a private package name that could also resolve from the public registry);
  Gradle repository order in `settings.gradle*` and missing `content { includeGroup(...) }` filters, `jitpack`, plain
  `http` repositories.
- **Build-time code.** Gradle plugins and `buildSrc`, Vite plugins, scripts that download and run tools
  (`curl ... | sh`), generated code committed without a way to regenerate and compare it.

## GitHub Actions
- `pull_request_target` or `workflow_run` that checks out the pull request's code and runs it (install, build, test)
  with secrets or a write token: anyone who opens a PR runs code with those secrets.
- Untrusted text in `run:` through `${{ }}`: PR title or body, branch name (`head_ref`), issue or comment text,
  commit messages. It becomes shell code; passing it through `env:` and quoting it avoids that.
- `permissions:` missing (the default token may be write) or wider than the job needs; a finding only with a reachable
  privileged action.
- Third-party actions on a moving tag vs a commit SHA; self-hosted runners on a public repository.
- Caches and artifacts written by a PR job and later restored by a release or deploy job.
- `workflow_dispatch` inputs used in commands or to pick a deploy target; secrets echoed, or written to files that
  become artifacts.

## Build context and shipped files
- Docker: the build context and `.dockerignore` (does `.env`, `storage/`, `.git`, a keystore end up in the image?);
  secrets passed as `ARG`/`ENV` or copied in an early stage stay in the image layers.
- Laravel `public/`: only intended files (no `phpinfo.php`, `.env` backups, test scripts); private uploads not under
  `storage/app/public`; production source maps are public (code visibility only, unless they hold secrets).
- Android release: keystores and their passwords out of the repo and CI logs; release built from a reviewed commit;
  Play App Signing setup is outside source.

## Releases, updates and plugins
Rare in our stacks; check only if present. A self-updater or plugin loader must verify a signature bound to the
product, version and channel, refuse downgrades, and fail closed when the signature, key or metadata is missing. A
release step should publish the exact artifact that was tested (same digest), not rebuild it from a moving tag.
