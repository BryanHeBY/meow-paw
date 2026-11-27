The Paw mark is an original vector drawing of the underside of a cat paw,
with four toe pads and a large central pad in the Meow palette.

The window loads `paw-mark.svg` directly. Linux StatusNotifier and the Windows
tray use straight-alpha RGBA buffers generated from the same SVG. With resvg
and ImageMagick installed, regenerate them from this directory:

```sh
preview_dir=$(mktemp -d)
resvg paw-mark.svg "$preview_dir/paw-mark.png"
for size in 16 24 32; do
  magick "$preview_dir/paw-mark.png" -filter Lanczos \
    -resize "${size}x${size}" -alpha on -depth 8 "RGBA:paw-mark-$size.rgba"
done
rm "$preview_dir/paw-mark.png"
rmdir "$preview_dir"
```
