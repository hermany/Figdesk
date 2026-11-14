#!/bin/sh
# Render the icon set from assets/app-icon/figcraft.svg with rsvg-convert (same outputs as `cargo xtask icon`).
set -e
cd "$(dirname "$0")/../.."
D=assets/app-icon; T=$(mktemp -d)
python3 - "$D/figcraft.svg" "$T/mac.svg" <<'PY'
import sys,re
s=open(sys.argv[1]).read()
vb=512/(824/1024); o=-(vb-512)/2
open(sys.argv[2],"w").write(s.replace('viewBox="0 0 512 512"',f'viewBox="{o:.3f} {o:.3f} {vb:.3f} {vb:.3f}"',1))
PY
for s in 16 32 48 64 128 256 512 1024; do
  rsvg-convert -w $s -h $s "$T/mac.svg" -o "$D/figcraft-$s.png"
  [ $s -le 512 ] && { mkdir -p "$D/hicolor/${s}x${s}/apps"; rsvg-convert -w $s -h $s "$D/figcraft.svg" -o "$D/hicolor/${s}x${s}/apps/ai.storyteller.figcraft.png"; }
done
for s in 16 20 24 32 40 48 64 128 256; do rsvg-convert -w $s -h $s "$D/figcraft.svg" -o "$T/ico-$s.png"; done
python3 - "$T" "$D/figcraft.ico" <<'PY'
import sys,struct
sizes=[16,20,24,32,40,48,64,128,256]
pngs=[open(f"{sys.argv[1]}/ico-{s}.png","rb").read() for s in sizes]
out=bytearray(struct.pack("<HHH",0,1,len(pngs))); off=6+16*len(pngs)
for s,p in zip(sizes,pngs):
    b=0 if s>=256 else s
    out+=struct.pack("<BBBBHHII",b,b,0,0,1,32,len(p),off); off+=len(p)
for p in pngs: out+=p
open(sys.argv[2],"wb").write(out)
PY
mkdir -p "$D/hicolor/scalable/apps"; cp "$D/figcraft-small.svg" "$D/hicolor/scalable/apps/ai.storyteller.figcraft.svg"
echo "icon set written to $D"
