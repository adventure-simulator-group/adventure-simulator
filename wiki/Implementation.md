### Organization
#### One-man-army (with support)
We believe that for whatever reason, indie gamedev attracts a sort of highly-generalized, highly-productive talent that can very efficiently solo large portions of a game's development. We want to harness this, and the rest of the team's job will be to support that developer. If they encounter a bug in a dependency that blocks them, someone else should fix it. If they need a plugin to be written, someone else should write it. And of course they shouldn't have to worry about things like accounting, legal, payroll, or whatever. But in general, this should be an arrangement where they are basically the only team member committing to the game's actual repository. They shouldn't have to think too hard about devops or spend a lot of the day coordinating with other developers. This plan is very much subject to change on the advice of whichever dev we hire for this position, but tentatively the organization might work like this:
##### The One-Man-Army (OMA)
Implements the game, can generally decide how to do so at their discretion
##### Adler Halbe
Handles admin and available to provide guidance/reviews/fix dependency bugs. Has a pretty good understanding of what everyone is working on, how it works, and can coordinate interfaces between plugins and the OMA. Adler is also an okay amateur 3D artist and can make placeholder assets as needed as well as animation graphs for a procedural animation system [based on Overgrowth's](https://youtu.be/LNidsMesxSE?si=BzSgxUV__CKHs70L). Adler will also make the website and maintain a developer wiki for documentation.
##### Bruno Segovia
Will be writing a procedural audio plugin. The goal is that instead of recording tons of sound effects, we generate them by writing them as functions with parameters. So for example, the length, diameter, and amount of gunpowder are parameters in a function that produces the sound of a gunshot. In the case of collisions the plugin can play most audio automatically (given that each colliding object have most or all of the info needed for this in its physics components like collider shape or restitution). Until Bruno succeeds with this, stock effects (or silence) are sufficient, therefore its not a blocker.
##### Graphics programmer
Instead of producing 3D assets the conventional way, Adler believes that we can do an at-least *okay* job using entirely procedural assets. Being highly flexible is more important than it looking realistic. Adler also has a couple hunches about ways of specifying and generating these assets that might be a lot better than current approaches, but he will need to consult with a graphics programmer to confirm. Until this is done, however, the OMA can simply make everything use colored primitives and/or Adler's placeholder assets.
### Technical
#### Stack
Language: Rust
Engine: Bevy
Physics: Avian
3D: Blender
Web: Rocket/maud
Knowledge: Obsidian
AI: Claude, or whatever the OMA wants