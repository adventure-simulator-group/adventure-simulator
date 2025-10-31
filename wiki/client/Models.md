To whatever extent possible, assets are as simple as possible. Essentially, we want this to be a game which is easy to add content to, and high-fidelity hand-made assets are a barrier to that. With that constraint however, we do still want it to look as good as we can. Hence, procedural models.

# Optimization
There is a single "smooth_theta" variable passed into the mesher that describes approximately the detail that it should be created at. The basis for it is essentially:
> *The theta in radians between the surface normals of any two faces on an ostensibly round surface.

So, for example, a value of PI/8 implies that a cylinder ought to have 16 vertices in its rings.

The value of smooth_theta depends on how far away the mesh is from the camera, how large its bounding box is, and screen resolution. It should be set so that when looking at a sphere, it is difficult to see the flat polygonal edges of it against the background. This also means there is a dynamic LOD system: meshes are regenerated if the distance gets, say, close enough that the ideal smooth_theta is half of the current value or so far that its twice the current value.
# Characters
## First pass: Collider-Based
The simplest possible mesh for a character is for it to conform precisely to the shape of the colliders of their bones. Lets say each bone is a capsule. Traverse through the skeletal hierarchy and generate a capsule mesh for each bone with the vertices all skinned to that bone.
## Second pass: Distance Fields and Vertex Weights
Next, we want to connect the capsule meshes. Connected bones' capsules are now 3D distance fields which combine with each other (this will smooth out the joints). There is a function to estimate how much influence a given point on the surface is getting from each bone, which determines its skinning weight.

We now have the basis for constructive solid geometry, because the 3D distance field capsule primitives give us a function to place vertices onto. We will place vertices and build triangles according to a modified version of advancing front:
### Polar-Space Advancing Front Tree
(rename to whatever you want)
Every vertex begins as a UV coordinate on the bone, with some arbitrary angle picked for the orientation of U = 0 (probably whatever places it at the back, assuming T-pose, like along the spine for the torso).
A bone is sort of between a capsule and a cylinder. V = 1 on a leaf bone (or V = 0 on the root bone) is always the center, like the apex of a capsule. But V = 1 on a bone that has another bone connected to its end has a defined U. There is essentially a V > 1 value due to the connection between the bones acting like a capsule, rather than a cylinder. You can think of this however you want, but essentially when two bones are 90 degrees from each other, the joint between them is still nice and spherical.

We begin at (0, 0) at the base of the pelvis and start constructing a triangle fan. The heuristic for placing vertices is based on the "smooth_theta" parameter, both for what V to put it at as well as what U. Once we have a fan, all of the vertices except for the apex are our advancing front.

There are two types of ways that the advancing front on a given bone handles connected bones. In the simple case, the other bone is a continuation of the shape of the current one. The relationship between the upper arm and forearm, or along spine bones, follows this pattern. In this case, the front seamlessly transitions between the bones using the pseudo-capsule method described above.

The second scenario is when you have bones that branch off of the current one. In this case, the front will essentially go around it by diverging at one of the vertices. The vertices in the gap created by this divergence are kept as a new, separate front which will be used later once the current bone or contiguous chain of bones has finished meshing (the mesher is depth-first). Eventually the front will pass completely over the gap and the divergence will be restored, also creating a continuous ring for the new front to begin from, repeating the advancing front process.

When the front reaches the distal end of a bone with no more connected bones, it must place an apex vertex and connect the front to it with a triangle fan.

Due to the fact that the front is advancing in UV space, not 3D space, an important optimization and simplification is available:
1. The heuristic for placing vertices can forbid placing any vertex to the left of a neighbor to its left, in UV space, or to the right of its neighbor to the right.
2. We assume that the hierarchy of bones are not self-intersecting
3. Because of this, in theory this can greatly speed up the algorithm since there's no need to test for intersections.

### But what about the shoulder?
Many a plan to procedurally generate a skinned character mesh has been defeated by the most infamous of joints. Essentially the idea is to forget about trying to weight it correctly or even do the topology correctly. Instead, after the mesh is generated (Lets just say in a T-pose, for example), we then apply an animation that lowers the arms, putting it in the worst-case scenario, but *then* calculate where each given vertex *would* be if it were placed on the surface again (as described above). Since these are distance fields, which combine in a smooth metaball-like fashion, the surface of the armpit will actually be quite a bit lower now as the arm and chest distance fields now nearly overlap.

There will no doubt need to be a lot of tweaking (perhaps the armpit is now *too* low) but this may just produce a mesh that looks more correct when the arms are down. Therefore we save this as a morph target, and animate it according to how much the upper arm bone is currently lowered. This can also be done to every joint, as even though none are quite as bad as the shoulder they still might benefit from it due to the fact that none of them will have particularly good topology or thoughtful vertex weights.
## Third-Pass: Heightmaps
This is the feature that enables characters, specifically body meshes, to actually look pretty realistic. As established, each bone has a UV semi-cylinder/semi-capsule space used for placing vertices. But we can reuse this as not only a universal space for textures, but to apply a heightmap to the distance field function that converts UV->3D.

Each bone has a heightmap defined in its UV space. The heightmap should actually encode the spherical curvature of the top of the head or tips of the fingers rather than actually treating them like capsules. It is still capsule-ish on connections between bones, however, to avoid looking jarring if they aren't perfectly aligned.

To actually produce the heightmaps to be used here, we can either bake them from a sculpt/scanned medical model or paint them with an in-game editor.
>I've experimented with both methods in Blender, they each work well enough - Halbe

Cruicially, we can also composite these heightmaps to produce many different character meshes. This has actually worked great in manual Blender experiments. Essentially there is a base heightmap representing a smooth, slender character, and a bone layer, muscle layer, and fat layer can be added on top of it. They are only additive with the base layer. Each layer is both a mask for the layer below and additive with it, which makes for a pretty good approximation of how they are physically layered in the body.

## Fourth-Pass: Face
We are under no illusion that one can use a system as simple as this to handle geometry like ears, nose, eyes, etc. Even a sharp chin will be a little awkward to handle. For the face, we can just use a system similar to what Nintendo used for Miis (and re-used for its recent Zelda games). The simplest version of that doesn't even include separate meshes for the facial features, they're just textures that get overlaid onto the face.

## Fifth-Pass: Clothing and Armor
We have an unambiguous, universal coordinate system for the surface of the body and a way to encode height. We can use the same mesher to produce meshes for body-conforming equipment entirely through texture data.

The main thing that is needed to support this is an alpha mask which specifies where the clothing is and isn't. So past, say, V=0.3 on the upper arms, for example, the value of this mask goes from 1 to 0 on a T-shirt. This can also be used to create holes in clothing, which the mesher treats similarly to intersections between bones.

Once the outer surface of a clothing mesh is completed, a [solidify algorithm](https://docs.blender.org/manual/en/latest/modeling/modifiers/generate/solidify.html) can be used to turn it into a proper model.

The heightmap for clothing is probably not necessarily with respect to the surface of the skin, otherwise a baggy shirt on a ripped guy will have abs, but with respect to a convex-only version of the base body mesh. Essentially, the base body's heightmap needs a version of it generated where any convex surface is pushed outwards until it is no longer convex.

Armor is similar to clothing, except in the case of non-flexible material like metal plates, all vertices need to have 1.0 weight with a single bone regardless of their location. Without an extremely detailed physics simulation this will mean lots of clipping, but this is acceptable.