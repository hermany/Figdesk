#!/usr/bin/env python3
"""Figdesk icon: the FigCraft chameleon silhouette as a halftone of dots, in a five-colour palette.
Reads assets/app-icon/figcraft-small.svg (the owner's drawing, MIT OR Apache-2.0), writes
assets/app-icon/figcraft.svg and figcraft-small.svg. Needs rsvg-convert and sips (macOS).
Then run `cargo xtask icon` (needs resvg) or figdesk/icon/render.sh to render PNG/ICO."""
import math, os, struct, subprocess, sys, tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SRC = os.path.join(ROOT, "assets/app-icon/figcraft-small.svg")
SRC_BACKUP = os.path.join(ROOT, "figdesk/icon/chameleon-source.svg")
N = 34                      # dots per side
S = 4                       # samples per cell side
BG = "#1e1e1e"
PALETTE = ["#f24e1e", "#ff7262", "#a259ff", "#1abcfe", "#0acf83"]  # red, orange, purple, blue, green

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
for gy in range(N):
    for gx in range(N):
        fig = ink = 0
        for sy in range(S):
            for sx in range(S):
                c = px(gx * S + sx, gy * S + sy)
                if not is_field(c):
                    fig += 1
                    ink += is_ink(c)
        cov = fig / (S * S)
        if cov < 0.2: continue
        light = (fig - ink) / (S * S)            # paper area -> big dots, engraved lines -> small
        r = cell * (0.20 + 0.32 * light * cov)
        cx, cy = (gx + .5) * cell, (gy + .5) * cell
        dots.append((cx, cy, r, colour((gx + gy) / (2 * (N - 1)))))

# The eye of the drawing, in 512 space: the engraving there is too fine to read as dots, so its dots are
# cleared and a ring of light dots with a dark pupil is drawn in its place.
EX, EY, ER = 237, 130, 40
dots = [d for d in dots if math.hypot(d[0] - EX, d[1] - EY) > ER + 12]
dots += [(EX + ER * math.cos(a * math.pi / 8), EY + ER * math.sin(a * math.pi / 8), 7.0, "#ffffff") for a in range(16)]
dots += [(EX + 21 * math.cos(a * math.pi / 3), EY + 21 * math.sin(a * math.pi / 3), 4.5, "#ffffff") for a in range(6)]
dots += [(EX, EY, 9.0, "#ffffff"), (EX, EY, 4.5, BG)]

def svg():
    out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><defs><clipPath id="t"><rect width="512" height="512" rx="112"/></clipPath></defs>'
           f'<g clip-path="url(#t)"><rect width="512" height="512" fill="{BG}"/>']
    out += [f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r:.1f}" fill="{c}"/>' for x, y, r, c in dots]
    out.append("</g></svg>")
    return "".join(out)
for name in ("figcraft.svg", "figcraft-small.svg"):
    open(os.path.join(ROOT, "assets/app-icon", name), "w").write(svg())
print(len(dots), "dots")
