```
struct StrataMap {
  map: [[SmallVec<Layer, 4>; 32]; 32]
  materials: SmallVec<Material, 4>
}
struct Layer { material_index: u4, depth: UnitScalar #0.0..1.0 }
enum Material {
  Skin { melanin: f32, carotin: f32, ???},
  Fat,
  Muscle,
  Bone,
  Steel,
  Paint { rgba: Rgba },
  Cloth,
  ...
}

impl Material {
  pub fn albedo(&self) -> f32,
  pub fn roughness(&self) -> f32,
  pub fn restitution(&self) -> f32,
  pub fn density(&self) -> f32,
  # ... other functions relevant for either graphics or physics
}
```


This tells you everything that you need to know about an entity's physical properties, given a shape and the UV map for its StrataMap. Its not *quite* the same as a regular UV map though, because it has to be able to also convert a "depth" to a point in 3D. The total depth of all layers is essentially a heightmap for the surface of a mesh, and generally only this and the outermost layer are relevant to the [model generator](Models.md). All of the layers are needed to compute physics properties, most of which are used at the client-layer for secondary physics, but the sum total of all mass from the StrataMap for all entities associated with a character can be relevant at the tactical-layer for [combat](Combat.md) equations or the strategic layer for calculating [fatigue](Energy.md) from [traveling](Travel.md) with [encumbrance](Encumbrance.md).

For the MVP, the function which generates the StrataMap for a character and their equipment is likely the responsibility of the programmer implementing the model layer since the information is most directly relevant to them. For the MVP, this all sounds fantastically complicated, so StrataMap can have a quick n' dirty temporary version that is just a wrapper around a single Material, forget the 2D map and layering, and combine a bunch of materials like Skin/Muscle/Fat/Bone into "Flesh" or different rock types into "Rock". The characters would basically look like 3D stick figures and the only weapons will be clubs and maces, but its fine for now.

The *eventual* purpose of StrataMap being so fantastically detailed is not only to be able to generate all of our models from it and have very accurate physics calculations, but it can also represent things like how the clothes and armor on a character (added on as layers) affect damage. An outer layer of steel followed by some padding would distribute force very effectively. Or a *gap* in the steel layer (for the slit of a visor) translates to a weak point that a character could attack with sufficiently high accuracy, with the animation system using inverse kinematics to place their weapon in the exact weakpoint. This enables the simulation at the tactical level to become extraordinarily detailed in the future after the bare minimum is implemented for the MVP.

In the MVP, there are only two shapes which a StrataMap is expected to support: the [pseudo-capsule](Models.md) and a flat terrain mesh. A shape which can support StrataMap is one which unambiguously maps a U, V, and depth value to an X, Y, and Z in an entity's local Transform space. There can be no UVDs which map to multiple XYZs or vice-versa.
>Adler: Bruno, what do you call this? Some geometric term that includes the word "transformation" I assume, like "functional transformation".

The following shapes should be able to facilitate this:
1. Cylinder (U: longitude, V: length, D: radius)
2. Capsule (U: longitude, V: length + top and bottom latitude, D: radius)
3. Sphere (U: longitude, V: latitude, D: radius)
4. Box (one face is chosen as the surface)
5. Blade
	1. May need two separate shapes for single-edge and double-edge
	2. There are a couple ways to map it, but it needs to support both pointed and flat heads