#!/usr/bin/env python3
"""Figdesk icon: the FigCraft chameleon silhouette as a halftone of dots, in a five-colour palette.
Reads assets/app-icon/figcraft-small.svg (the owner's drawing, MIT OR Apache-2.0), writes
assets/app-icon/figcraft.svg and figcraft-small.svg. Needs rsvg-convert and sips (macOS).
Then run `cargo xtask icon` (needs resvg) or figdesk/icon/render.sh to render PNG/ICO."""
import math, os, random, struct, subprocess, sys, tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SRC = os.path.join(ROOT, "assets/app-icon/figcraft-small.svg")
SRC_BACKUP = os.path.join(ROOT, "figdesk/icon/chameleon-source.svg")
N = 40                      # dots per side
S = 10                      # samples per cell side
BG = "#12d6c2"     # Figma green, pushed toward cyan
INK = "#1e1e1e"
rng = random.Random(7)      # fixed seed: the scatter is random-looking but the icon is reproducible
PALETTE = ["#f24e1e", "#ff7262", "#a259ff", "#3d5afe", "#14308f"]  # red, orange, purple, indigo, deep blue (light cyan/green would vanish on the background)

if not os.path.exists(SRC_BACKUP):               # keep the original drawing so the script can re-run
    open(SRC_BACKUP, "wb").write(open(SRC, "rb").read())
size = N * S
with tempfile.TemporaryDirectory() as t:
    subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size), SRC_BACKUP, "-o", t + "/a.png"], check=True)
    subprocess.run(["sips", "-s", "format", "bmp", t + "/a.png", "--out", t + "/a.bmp"], check=True, capture_output=True)
    d = open(t + "/a.bmp", "rb").read()
off = struct.unpack_from("<I", d, 10)[0]
w, h = struct.unpack_from("<ii", d, 18); bpp = struct.unpack_from("<H", d, 28)[0]
bp = bpp // 8; row = (w * bp + 3) // 4 * 4; top_down = h < 0; h = abs(h)
def px(x, y):
    o = off + (y if top_down else h - 1 - y) * row + x * bp
    return d[o + 2], d[o + 1], d[o]
def in_tile(x, y):   # inside the source tile's rounded corners (its corners are not the drawing)
    k = 0.22 * size; dx = max(k - x, x - (size - 1 - k), 0); dy = max(k - y, y - (size - 1 - k), 0)
    return dx * dx + dy * dy <= k * k
def is_field(c):   # the tile's green field
    return abs(c[0] - 2) + abs(c[1] - 218) + abs(c[2] - 150) < 90
def is_ink(c): return sum(c) < 220

def lerp(a, b, t): return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))
def hexrgb(s): return tuple(int(s[i:i + 2], 16) for i in (1, 3, 5))
def colour(u):     # u in 0..1 along the diagonal, smooth blend through the palette
    u = min(max(u, 0), 1) * (len(PALETTE) - 1); i = min(int(u), len(PALETTE) - 2)
    r = lerp(hexrgb(PALETTE[i]), hexrgb(PALETTE[i + 1]), u - i)
    return "#%02x%02x%02x" % r

cell = 512 / N
dots = []
cells = set()
for gy in range(N):
    for gx in range(N):
        fig = ink = 0
        for sy in range(S):
            for sx in range(S):
                c = px(gx * S + sx, gy * S + sy)
                if in_tile(gx * S + sx, gy * S + sy) and not is_field(c):
                    fig += 1
                    ink += is_ink(c)
        cov = fig / (S * S)
        if cov < 0.12: continue
        cells.add((gx, gy))
        light = (fig - ink) / (S * S)            # paper area -> big dots, engraved lines -> small
        r = cell * min(0.58, (0.36 + 0.24 * (ink / max(fig, 1)) ** 0.6) * rng.uniform(0.85, 1.15))   # darker engraving -> bigger dot
        cx = (gx + .5 + rng.uniform(-0.1, 0.1)) * cell                    # scattered off the grid
        cy = (gy + .5 + rng.uniform(-0.1, 0.1)) * cell
        u = (gx + gy) / (2 * (N - 1)) + rng.uniform(-0.14, 0.14)
        dots.append((cx, cy, r, colour(u), (gx, gy)))

def darken(c, k=0.8):
    return "#%02x%02x%02x" % tuple(round(v * k) for v in hexrgb(c))
def edge(g):      # touches an empty cell: the outline of the drawing
    return any((g[0] + dx, g[1] + dy) not in cells for dx in (-1, 0, 1) for dy in (-1, 0, 1))
dots = [(x, y, r, darken(c) if edge(g) else c) for x, y, r, c, g in dots]

EX, EY, ER = 237, 130, 40       # the eye of the drawing, in 512 space: cleared, then a ring of dark dots and a pupil
dots = [d for d in dots if math.hypot(d[0] - EX, d[1] - EY) > ER + 14]
dots += [(EX + ER * math.cos(a * math.pi / 8), EY + ER * math.sin(a * math.pi / 8), 7.5, "#1e1e1e") for a in range(16)]
dots += [(EX + 20 * math.cos(a * math.pi / 3), EY + 20 * math.sin(a * math.pi / 3), 5, "#1e1e1e") for a in range(6)] + [(EX, EY, 7.5, "#1e1e1e")]

def svg():
    out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><defs><clipPath id="t"><rect width="512" height="512" rx="112"/></clipPath></defs>'
           f'<g clip-path="url(#t)"><rect width="512" height="512" fill="{BG}"/>']
    out += [f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r:.1f}" fill="{c}"/>' for x, y, r, c in dots]
    out.append("</g></svg>")
    return "".join(out)
for name in ("figcraft.svg", "figcraft-small.svg"):
    open(os.path.join(ROOT, "assets/app-icon", name), "w").write(svg())
print(len(dots), "dots")
