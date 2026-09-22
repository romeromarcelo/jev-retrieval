---
name: using-jevr
description: >
  Semantic codebase search with the jevr CLI — finds the code or document
  files matching a concept from a natural-language query, in place of
  grep+glob for codebase navigation. Use when locating where something is
  implemented or documented ("where is X implemented?", "which files handle
  Y?", "where is Y explained?"), finding relevant files or snippets for a
  task, tracking down a specific function/class/feature whose location is
  unknown, or searching code or prose (markdown docs, notes, specs) by
  meaning rather than exact strings — even when the user doesn't name the
  tool.
allowed-tools: Bash(jevr:*)
---

# Using jevr

`jevr` answers *"where is the code that does X?"* — and equally *"where is X
documented?"* — with a ranked list of files and line ranges. Query in plain
language; get back calibrated relevance scores, not keyword hits.

## Canonical invocation

```
jevr "<natural-language question>" [PATH]
```

`PATH` is a directory **or a single file** (default `.`), just like grep.

```bash
# Code: map a concept to files
jevr "where is retry backoff implemented for dropped connections?" src/

# Documents: same command, point PATH at prose
jevr "which doc explains the release process?" docs/

# Single file: locate the relevant section of one large file
jevr "where are request timeouts configured?" src/server/config.rs

# Pipelines are safe (output ends cleanly on early exit)
jevr "authentication middleware" . | head -5
```

## jevr or grep?

- **Concept known, wording unknown** ("which files handle session expiry?") → `jevr`
- **Exact string, regex, or known symbol name** → `grep`/`rg`
- They compose: one jevr query with `--snippets` reveals the real symbol
  names; one precise grep then finds every occurrence. That beats a chain of
  speculative `rg -i "a|b|c"` alternations. If your grep alternations keep
  growing, you're searching for a concept — switch to jevr.

## Reading the output

Default output is one Read-ready target per line, best first:

```
src/pricing.py:321-420  0.92
```

`path:start-end  score` — the best-matching window's line range and a
calibrated 0..1 relevance score. **Next step: open the returned range with
the Read tool** (`offset=start`, `limit=end-start+1`).

- `--snippets` — also prints each file's best window with `N:`-prefixed line
  numbers, capped at 25 lines, ending in `... N more lines (Read path:A-B
  for the rest)` when cut.
- `--json` — full verdicts as a JSON array for programmatic use (`--top-k`
  still applies).

### Exit codes (grep convention)

| Exit | Meaning |
|------|---------|
| 0 | Matches at or above threshold printed |
| 1 | Nothing cleared the threshold; the best below-threshold guesses may still print with a stderr notice. Not a failure — a low best score (~0.3) is strong evidence the concept isn't there; a near-miss (~0.85) says rephrase or lower `--threshold` |
| 2 | Usage/config error (bad flags, missing `TYPESAFE_API_KEY`) |

### Two result lanes

Code files keep score ≥ 0.90; text documents keep ≥ 0.60 (prose scores run
lower for equally relevant content). Document results always print **after**
code results, each lane ranked and capped at `--top-k` independently. Seeing
0.9x code lines followed by 0.6x document lines is the document lane's own
threshold at work, **not** broken filtering — whenever documents print, one
stderr line spells out the split:

```
jevr: 7 code result(s) kept at score >= 0.90, then 3 document result(s) kept at score >= 0.60
```

## Key flags

| Flag | Default | Purpose |
|------|---------|---------|
| `--snippets` | off | Inline best-window preview per file |
| `--json` | off | Machine-readable verdicts |
| `--top-k <N>` | 10 | Max files printed, per lane |
| `--threshold <F>` | 0.90 code / 0.60 docs | Minimum score to keep; one explicit value sets both lanes — lower it to probe weak signals |
| `--no-cache` | off | Bypass the response cache (every request billed) |

## Pitfalls

```bash
# ✅ Phrase queries as questions/concepts, the way you'd ask a colleague
jevr "how does the system reconnect a dropped websocket?"

# ❌ Keyword lists rank measurably worse
jevr "websocket, reconnect, retry, backoff"

# ❌ Don't add -k keyword variants — measured worse than the bare query
jevr "reconnect logic" -k "websocket,retry"

# ❌ Queries are plain language, not regex — no escaping, no alternations
jevr "retry|backoff|reconnect"
```

## Iterative narrowing

```bash
# 1. Start repo-wide with the plain question
jevr "where is report verification implemented?"

# 2. Exit 1? Probe the weak-signal landscape
jevr "where is report verification implemented?" --threshold 0.3 --top-k 15

# 3. Narrow scope as understanding grows: subtree → file (+ snippets)
jevr "report signature verification" crates/verifier --snippets

# 4. Read the winning ranges; grep the exact symbols the snippets revealed
```

## Requirements

- `jevr` binary on PATH (`cargo install --path .` from the repo).
- `TYPESAFE_API_KEY` exported. Cold queries are billed network work (a few
  seconds, ~cents); responses are cached under `~/.cache/jevr/`, so repeat
  queries over unchanged code are instant and free — iterate freely.
- gitignore is respected automatically; no `git ls-files` plumbing needed.
