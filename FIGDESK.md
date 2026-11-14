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
