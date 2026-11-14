# Figdesk

Public fork of [FigCraft](https://github.com/storytold/figcraft) (Apache-2.0 OR MIT).
Upstream stays the base; Figdesk adds features on top. Original licenses and notices are kept.

## Our additions (kept in their own paths to limit merge conflicts)
- `figdesk/ai-panel/` — chat panel that edits the open document live through FigCraft's control server (`--control PORT`).
- `figdesk/icon/` — dotted-chameleon app icon generator (`make_icon.py`, `render.sh`); outputs replace the files in `assets/app-icon/`. After an upstream merge, if upstream changed the icon, re-run `make_icon.py` and `render.sh`.
- `figdesk/scripts/upstream-compare.sh` — compares us with upstream.

## Sync routine (every time FigCraft updates)
1. `figdesk/scripts/upstream-compare.sh` — new upstream commits, changed areas, likely conflicts.
2. Review what upstream added; decide what to adopt, what we already do better, what to improve.
3. `figdesk/scripts/upstream-compare.sh --merge` (or `git merge upstream/main`), fix conflicts, build, push.

Rule: change upstream files only when needed; prefer new files/crates under `figdesk/` or clearly marked hooks.

## Agent harness (Figdesk)

`CLAUDE.md` loads `AGENTS.md` (upstream rules, unchanged) and then this file. Where they differ, this
file only adds Figdesk-specific constraints; it never relaxes an upstream rule.

### Start every session
1. Read `ROADMAP.md` and `docs/gaps.md` (upstream work list), then the "Our additions" list above.
2. Run `git remote -v`: `origin` is the Figdesk fork, `upstream` is `storytold/figcraft`. If `upstream`
   is missing, `figdesk/scripts/upstream-compare.sh` adds it.
3. If the task touches upstream code, check `figdesk/scripts/upstream-compare.sh` first: someone may
   have fixed it upstream already, and a local fix there is a future merge conflict.

### Where code goes
| Change | Location | Notes |
|---|---|---|
| Figdesk-only feature | `figdesk/<feature>/` | Own folder, own README. Preferred. |
| Engine/UI behaviour | upstream crates (`crates/*`) | Only when needed. Keep the diff small and mark hooks with a `Figdesk:` comment so merges are easy to review. |
| Icon / branding | `figdesk/icon/` then regenerate into `assets/app-icon/` | Add or update the `ATTRIBUTION.md` row in the same commit. |
| AI panel | `figdesk/ai-panel/` | Talks to FigCraft only through the control server; no engine changes. |

### Rules specific to the fork
- **Ask before changing a shared upstream surface**: `crates/engine` command ids (`catalog.rs`), the
  control protocol (`docs/control-protocol.md`) and `xtask`. Explain what changes and why, then wait.
  The AI panel and other tools depend on those ids and methods.
- **Every new command is reachable from the control channel.** The AI panel only sees what
  `engine.execute` can run, so a feature that is menu-only is invisible to it.
- **No new dependencies without a licence check** against the allowlist in `AGENTS.md`. The panel is
  Node/JS: it is a fork tool outside the native app, so the "Rust only" rule does not apply to
  `figdesk/ai-panel/`, but nothing from it may ship inside the native binary or the WASM build.
- **Secrets**: `ANTHROPIC_API_KEY` is read from the environment only. Never write it to a file, a log or
  a screenshot. Do not commit `.env` files.
- **Upstream sync is its own task.** Do not mix a merge with feature work in one commit.

### How to run and verify
```sh
cargo run -p figcraft -- --control 47821 [file.figcraft]   # app with the control channel
FIGCRAFT_BIN=target/debug/figcraft ./figdesk/ai-panel/start.sh   # panel against a local build
cargo xtask ci                                             # gate before every commit
```
`start.sh` defaults to the installed app; set `FIGCRAFT_BIN` to use a local build. For UI changes,
drive the app over the control channel, take a `ui.screenshot` and read the PNG before claiming it works.
For the panel: start it, send a prompt that edits the selection, confirm the canvas changes, and check
the browser console for errors.

### Commits
One task per commit, milestone id in the message (`M2.1: ...`) for upstream-area work, `Figdesk: ...`
for work under `figdesk/`. Update the affected docs row in the same commit.

### Registry (keep current)
When you add one of these, add its row here in the same commit; when you remove it, delete the row.

| Item | Path | Purpose |
|---|---|---|
| AI panel | `figdesk/ai-panel/` | Claude chat that edits the open document live |
| Icon generator | `figdesk/icon/` | Dotted-chameleon app icon |
| Upstream compare | `figdesk/scripts/upstream-compare.sh` | New upstream commits, changed areas, likely conflicts |
