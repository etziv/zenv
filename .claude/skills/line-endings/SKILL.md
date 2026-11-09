---
name: line-endings
description: "Write or rewrite a file from a script on Windows (Python open/Path.write_text/print redirect/csv, PowerShell Set-Content/Out-File/Add-Content/here-strings), or anything shows mixed endings: git ls-files --eol says w/crlf or w/mixed, a diff is whole-file instead of a few lines, ^M in a file, a .sh fails with $'\r': command not found, or a lone \r mid-line. Use before the write, or to check/fix one already committed."
---

# Line endings

## Rule
Every file you write is LF without BOM, except `*.ps1`, `*.cmd`, `*.bat` (CRLF; the kit's `.gitattributes`
has `* text=auto eol=lf` plus those three). Vendored data that arrived CRLF keeps its endings: do not
renormalize files you did not change. `.gitattributes eol=lf` only normalizes what `git` checks out/commits — it
never touches the file already sitting in your working tree. A script you run, or a container that mounts the
working tree, reads the bytes on disk as they are; a `.sh` with CRLF fails in Linux bash (servers, Docker) with
`$'\r': command not found`, although Git Bash on Windows runs it, so a local run proves nothing.

## Verified on this machine (Python 3.14, PowerShell 5.1, Git Bash, node) — write LF the first time
| Tool / call | Result | Use instead |
|---|---|---|
| Python `open(p, 'w')`, `Path.write_text(s)`, `print(x, file=f)` | **CRLF** (default text mode translates `\n`) | `open(p, 'w', newline='\n')` or `Path.write_text(s, newline='\n')` |
| Python `csv.writer` | **CRLF even with `newline=''`** — csv's own default `lineterminator` is `\r\n` | `csv.writer(f, lineterminator='\n')` on a `newline=''`-opened file |
| Python `print(...)` with stdout redirected (`python x.py > f`) | **CRLF** | `sys.stdout.reconfigure(newline='\n')` first, or write the file directly |
| PowerShell `@("a","b") \| Set-Content` (array input, the common pattern) | **CRLF** between and after lines | `[IO.File]::WriteAllText(...)` below |
| PowerShell `"a`nb" \| Set-Content` (one string, no `-NoNewline`) | **mixed**: LF inside, CRLF appended at the end | same |
| PowerShell `Set-Content -NoNewline -Encoding utf8` | LF, but 5.1 writes a **UTF-8 BOM** (`EF BB BF`) | same |
| PowerShell 5.1 `Out-File` / `>` | **CRLF + BOM**: UTF-8 BOM in the agents' PowerShell tool (its profile sets `Out-File:Encoding=utf8`), UTF-16LE with `-NoProfile` | same |
| `[IO.File]::WriteAllText("<absolute path>", $text)` | **LF, UTF-8 without BOM** (no translation) | the PowerShell recipe; build `$text` with `` `n ``; a relative path resolves against the process dir, not `$PWD` |
| node `fs.writeFileSync(p, s)` | LF preserved (no translation) | fine |
| Write / Edit tool | LF | fine |
| Git Bash `sed -i` on an already-LF file | LF preserved | fine |

General recipe: build the string with `\n` only, then write with a call that does not translate
(`newline='\n'` or `newline=''`+explicit `lineterminator` in Python; `[IO.File]::WriteAllText` in
PowerShell; everything else above is already safe). Simplest on this machine: write with the Write/Edit tool or node.

## Check
```
git ls-files --eol | grep -E 'w/(crlf|mixed)'      # working-tree endings per file, vs git's own i/ (index)
grep -lU $'\r' <files>                              # any CR at all (fast, broad sweep; flags normal CRLF too)
grep -UPzq '\r(?!\n)' <file>                        # true lone CR only (exit 0 = found) — needs BOTH -U and -z:
                                                     # -U alone still strips the line's \n before matching, so
                                                     # even ordinary CRLF lines falsely match; -z keeps the
                                                     # whole file as one buffer so the lookahead sees the real \n
```
A lone CR (e.g. from `\r` typed into a template string, or a `.ps1` that wrote `` `r `` without
`` `n ``) shows as no highlighted `^M` in some editors but still breaks line-based tools; always run
the `-UPz` form (verified above — plain `-UP` gives false positives on normal CRLF) on a file you
suspect, not just a CR count.

## Fix
```
# one file, in place, keep everything else identical
sed -i 's/\r$//' path/to/file          # Git Bash: strip trailing CR (CRLF -> LF)
# or rewrite from a known-good buffer with the recipes above (Python newline='\n', PowerShell -join)
```
Then re-run the check above and `git diff --stat` to confirm only line endings changed (no stray
content diff).
