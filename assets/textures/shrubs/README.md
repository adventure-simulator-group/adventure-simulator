# Common hazel leaf material

The common-hazel (`Corylus avellana`) material uses one registered canonical
leaf plate for every PBR channel. `source/common_hazel_leaf_plate.png` was
generated as a neutral orthographic morphology plate after comparison with the
CC0 photograph *Corylus avellana young leaves.JPG* by Wikimedia Commons user
Аимаина хикари:

- https://commons.wikimedia.org/wiki/File:Corylus_avellana_young_leaves.JPG
- CC0 1.0 Universal: https://creativecommons.org/publicdomain/zero/1.0/

The generated source plate itself is contributed to Fabelgeist under the
repository licence. It is not a redistributed copy of the reference photo.

Run `powershell -ExecutionPolicy Bypass -File
scripts/build_hazel_leaf_textures.ps1` from the repository root to regenerate
the aligned opacity, front/back albedo, height, and DirectX normal maps. The
script segments, de-spills, and derives all channels from that exact source
pixel grid; it does not generate independent images that could drift out of
registration.
