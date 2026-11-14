#!/bin/sh
# Compare Figdesk with upstream FigCraft (storytold/figcraft).
# Usage: scripts/upstream-compare.sh [--merge]
set -e
cd "$(git rev-parse --show-toplevel)"
git remote get-url upstream >/dev/null 2>&1 || git remote add upstream https://github.com/storytold/figcraft.git
git fetch upstream --tags -q
BASE=$(git merge-base HEAD upstream/main)
echo "== New upstream commits since last sync ($(git rev-list --count HEAD..upstream/main)) =="
git log --oneline --no-merges HEAD..upstream/main | head -50
echo; echo "== Files changed upstream (by top-level area) =="
git diff --stat "$BASE" upstream/main | tail -1
git diff --name-only "$BASE" upstream/main | cut -d/ -f1-2 | sort | uniq -c | sort -rn | head -15
echo; echo "== Our additions on top of upstream =="
git diff --stat upstream/main...HEAD | tail -5
echo; echo "== Likely conflicts =="
git merge-tree --write-tree --name-only HEAD upstream/main 2>/dev/null | sed -n '2,$p' | head -20 || true
[ "$1" = "--merge" ] && git merge upstream/main
exit 0
