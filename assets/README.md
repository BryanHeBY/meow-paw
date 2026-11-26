The Paw mark is an original SVG using Meow's blue, ivory, and warm yellow
palette as visual reference; no upstream app artwork is embedded.

`paw-mark.svg` is the source. Rasterize at 256 × 256 using resvg, then generate
the straight-alpha RGBA tray variants with:

```sh
resvg paw-mark.svg paw-mark.png
for size in 16 24 32; do
  magick paw-mark.png -filter Lanczos -resize "${size}x${size}" -depth 8 "RGBA:paw-mark-$size.rgba"
done
```
