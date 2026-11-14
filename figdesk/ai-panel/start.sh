#!/bin/sh
# Opens FigCraft with its control server, then the AI panel.
PORT=${FIGCRAFT_CONTROL_PORT:-47821}
cd "$(dirname "$0")"
[ -d node_modules ] || npm install
${FIGCRAFT_BIN:-/Applications/FigCraft.app/Contents/MacOS/FigCraft} --control "$PORT" "$@" >/dev/null 2>&1 &
sleep 2
( sleep 1; open "http://127.0.0.1:${PORT_UI:-4848}" ) &
FIGCRAFT_CONTROL_PORT=$PORT PORT=${PORT_UI:-4848} node server.mjs
