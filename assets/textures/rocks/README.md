# Rock surface PBR channels

These three matched 1K JPEG channels come from Poly Haven's
[Rock Surface](https://polyhaven.com/a/rock_surface), authored by Amal Kumar.
The source is a two-metre-wide weathered rock scan and is published under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).

| File | SHA-256 |
| --- | --- |
| `rock_surface_diff_1k.jpg` | `07edda2bbe4715b01aae2e4bac9f21f204cf4aae37b4733ef01de33281e5e60e` |
| `rock_surface_nor_gl_1k.jpg` | `c645af7f6a20bf77421765e6f998873834821e1950a3a0a21c5ee9ced32cecc1` |
| `rock_surface_arm_1k.jpg` | `8de45a3aa0be70f8baf5c609603bb6bba0274ee4eb8c8e47dad455258d5d7f2d` |

The normal and ARM images must be loaded as linear data. The diffuse image is
sRGB. All three use the same metric triplanar projection in the tactical rock
material so their features remain aligned without requiring authored UVs.
