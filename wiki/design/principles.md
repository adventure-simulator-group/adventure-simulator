---
status: draft
author: Bruno Segovia
scope: Project-wide product and design philosophy
content_type: explanation
---

# Design principles

This page outlines *Fabelgeist*'s design principles. Read it before
contributing! All other pages are subordinate to this one.

## Project promise

*Fabelgeist* should make its players feel like adventurers in a fantastical,
believable, and shared world.

"Shared" is the operative word. Though the game will be fun played alone, it is
fundamentally a social game, and its greatest moments should come from players'
interactions with other players—the bonds and rivalries formed between strangers
in a strange land—and their lasting impacts on the world. What we do for world
design, we do to give these interactions *context*; what we do for game
mechanics, we do to give these interactions *consequences*.

## The browser is the medium

*Fabelgeist* is a game about interacting with strangers over the Internet. As
far as we know, there is no better tool for interacting with strangers over the
Internet than the web browser. To take full advantage of this , the game should
be not only [browser-based](https://en.wikipedia.org/wiki/Browser_game) but also
browser-*shaped*:

* Players should be able to navigate the world like they navigate the Internet,
  following [hyperlinks](https://en.wikipedia.org/wiki/Hyperlink) between
  people, places, organizations, quests, rumors, objects, and ideas.
  * In a sense, this should be a self-documenting game; it is its own wiki.
* Anything in-world which players can meaningfully discuss should be something
  they can reference as a hyperlink.
* People should be able to play the whole game in the asynchronous, web-based
  ["strategic"](#strategic-and-tactical-layers) layer. Though immediate dangers
  invite real-time tactical simulation, they may be autoresolved; tactical play
  offers an optional close-up of select events, which may be preferred, but it
  is never required for progress.

In short, this isn't a "normal" web game that runs in a client inside the
browser, like old *Minecraft*, but a fully
[hypertext](https://en.wikipedia.org/wiki/Hypertext) web game which can *become*
a normal game when desired, like in combat.

Since people already use the browser every day—for work, for emails, for social
media—the game should slot right into their existing workflows. We're like any
other tab in your browser, just a pretty fun one. We're also like any other game
in your library, just one you can switch out of when your boss walks by. No
longer must you boot up Steam and lock your computer into a fullscreen
executable for a couple hours just to enjoy a quality video game; that is a
relic of a bygone age, friends, an age in which your browser couldn't *handle*
high-quality, real-time 3D graphics. But novel web technologies
([Wasm](https://webassembly.org/),
[WebGPU](https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API)) have
brought that age to a close, and no one has even realized it! No one has even
realized it... yet.[^1]

## One world, many stories

<!-- ... -->

## Strategic and tactical layers

<!-- Explain why play is strategic by default and when tactical simulation earns its cost. -->

## Physically based gameplay

<!-- Define the causal-model principle, including abstraction and fantastic exceptions. -->

## Familiar fantasy

<!-- Define the intended relationship among familiar archetypes, historical texture, and originality. -->

## Contributor-friendly production

<!-- Define the preference for reproducible systems, data, generators, and accessible contribution. -->

## Principles and accepted details

<!-- Explain how durable principles relate to binding but revisable feature contracts. -->

[^1]: We aren't entirely without predecessors. [*Fallen London*](https://www.fallenlondon.com/) is a text-based browser RPG which, to quote *PC Gamer*, "will ease itself into your spare tabs, onto your phone and behind your eyelids"; [*Prosperous Universe*](https://prosperousuniverse.com/about) is an asynchronous browser MMO driven by a persistent player economy; and [*Blaseball*](https://www.thegameband.com/game/blaseball) was a browser-based baseball simulation whose players gave it an extensive, shared mythology. Each fulfills some part of the *Fabelgeist* mission, but not all of it. More pertinently, we don't know of any browser games which combine a hypertextual strategic world with a real-time adventure game with high-performance 3D graphics; *that* feature, likely our main technological draw, only became practical in [late 2025](https://web.dev/blog/webgpu-supported-major-browsers), once WebGPU began shipping with all major browsers.
