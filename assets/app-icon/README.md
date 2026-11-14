# Figdesk app icon (dotted chameleon)

**Figdesk change:** the icon is the FigCraft chameleon silhouette rebuilt as a halftone of dots (about 750
circles) on a dark tile, coloured with a five-hue diagonal gradient (`#f24e1e #ff7262 #a259ff #1abcfe #0acf83`).
It is generated from the owner's original drawing by `figdesk/icon/make_icon.py` (source kept in
`figdesk/icon/chameleon-source.svg`) and rendered by `figdesk/icon/render.sh`. No third-party logo shapes are used,
only plain colours. The text below describes the upstream FigCraft icon.


A **chameleon**: an engraving portrait in profile with a curled tail, in the Crafting Apps'
owl-template framing (full-bleed field, head and body filling the tile, body running off the edges).

## Palette

Exactly three colours:

| Colour | Hex | Used for |
|---|---|---|
| Ink | `#0b0b0c` | line work and contours |
| Paper | `#efe9dc` | the figure |
| FigCraft green (app colour) | `#02da96` | the full-bleed field |

## Geometry

A 512-unit tile (`viewBox="0 0 512 512"`), rounded square with `rx=112`, no border. The macOS
renders pad it onto Apple's 824/1024 icon grid; Windows and Linux use the tile edge to edge, with
its rounded corners, like the other Crafting Apps.

## Provenance

The owner's original engraving-style drawing (1500 px, keyed to the palette), supplied for
FigCraft and vectorised. The source
image is not kept in this repo. License: see `LICENSE.txt`.

## Files

- `figcraft.svg`: canonical master (traced at 1500 px, the source's size).
- `figcraft-small.svg`: lighter trace (traced at 1024 px); also the hicolor scalable icon.
- `figcraft-<size>.png` (16–1024): renders on the macOS grid; `packaging/macos/package.sh` builds
  the `.icns` from them.
- `figcraft.ico`: Windows icon, 16–256 px, embedded in the `.exe` by `apps/figcraft/build.rs`.
- `hicolor/<size>/apps/ai.storyteller.figcraft.png` (16–512) and `hicolor/scalable/...svg`:
  Linux icon theme; the 256 px one is also the runtime window icon off macOS.

## Regenerate

Replace `figcraft.svg` (and `figcraft-small.svg`), then run `cargo xtask icon`. It needs `resvg`
(`cargo install resvg`).
