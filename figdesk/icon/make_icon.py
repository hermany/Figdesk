#!/usr/bin/env python3
"""Figdesk icon: FigCraft's own chameleon drawing (the owner's, MIT OR Apache-2.0), unchanged, with a new
background and the five colours of the Figma logo through the animal. The drawing is a green tile, a cream
shape (the paper) and a black one (the engraving): the tile and the cream are recoloured, the engraving stays.
Reads figdesk/icon/chameleon-source.svg (FigCraft's icon as it was), writes assets/app-icon/figcraft.svg and
figcraft-small.svg. Then run figdesk/icon/render.sh (or `cargo xtask icon`) for the PNG and ICO files."""
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SRC = os.path.join(ROOT, "figdesk/icon/chameleon-source.svg")
BG = "#000000"
PALETTE = ["#f24e1e", "#ff7262", "#a259ff", "#1abcfe", "#0acf83"]  # red, orange, purple, blue, green

# Original colours of the drawing.
TILE, PAPER = "#02da96", "#efe9dc"

svg = open(SRC).read()
for old in (TILE, PAPER):
    assert svg.count(f'fill="{old}"') == 1, f"{old} should colour exactly one shape"
stops = "".join(f'<stop offset="{i / (len(PALETTE) - 1):.2f}" stop-color="{c}"/>' for i, c in enumerate(PALETTE))
gradient = f'<linearGradient id="figma" gradientUnits="userSpaceOnUse" x1="40" y1="40" x2="472" y2="472">{stops}</linearGradient>'
svg = svg.replace("<defs>", f"<defs>{gradient}", 1)
svg = svg.replace(f'fill="{TILE}"', f'fill="{BG}"', 1).replace(f'fill="{PAPER}"', 'fill="url(#figma)"', 1)
for name in ("figcraft.svg", "figcraft-small.svg"):
    open(os.path.join(ROOT, "assets/app-icon", name), "w").write(svg)
print("recoloured", len(svg), "bytes")
