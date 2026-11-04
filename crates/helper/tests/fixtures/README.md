# Generated JPEG DC fixtures

Original synthetic RGB gradient created by this project; no external photo/data.

`magick -size 64x48 gradient:#d04020-#2060e0 -sampling-factor 2x2 -interlace Plane -quality 91 progressive-gradient-64x48.jpg`

Reference raw RGB from installed libjpeg-turbo djpeg3.2.0:
`djpeg -scale 1/8 -nosmooth -rgb progressive-gradient-64x48.jpg`
P6header removed,8×6×3bytes. Functional pixel comparison only, no performance gate.

4:4:4 variant uses `-sampling-factor 1x1`, otherwise identical commands.
Djpeg uses2×2chroma IDCT for4:2:0 at1/8 output; vendor uses1×1 everycomponent.
Use exact full-vs-sparse parity for4:2:0, external reference tolerance3 only
for4:4:4 with matching component scales. See reference-component-scales.json.

PNG Adam7 original synthetic31×23 RGBA8 (no external image):
`magick -size 31x23 gradient:#e04020-#2050e0 -fill '#60c080a0' -draw 'rectangle 0,0 12,8' -alpha set -depth 8 -define png:color-type=6 -interlace PNG adam7-rgba-31x23.png`
