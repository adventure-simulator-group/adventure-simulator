# Wiki writing

> **Status:** Canonical guidance
>
> **Author:** Bruno Segovia
>
> **Scope:** Wiki-wide writing guidelines

This page outlines the writing guidelines for the *Fabelgeist* Wiki. Read it
before contributing to the wiki!

<!-- toc -->

## Audience and voice

We write this wiki for current and prospective players and contributors.[^5]

For editorial purposes, when writing for this wiki, imagine you are speaking
with a respected, intelligent colleague who knows nothing about your project
but is eager to learn more. In fact, imagine you are speaking to an idealized
version of your coworker,[^1] to whom we will refer as Your Coworker, with the
following properties:

*   Your Coworker can read at a collegiate level, but she'd rather you keep it
    high-school where you can.
*   Your Coworker enjoys your natural voice. That is, she is not in HR or
    marketing, so she isn't expecting a veneer of corporate neutrality, and she
    is unlikely to enjoy reading it.[^2]
*   Your Coworker enjoys humor that earns its keep, which is to say humor that
    is dry and employed sparingly. Your Coworker does not[^3] enjoy Marvel
    movies.
    *   Your Coworker may enjoy *light* mean-spirited humor if it helps you
        prove a point, but you should keep it light, and importantly, try to
        direct it at something that is not Your Coworker. She is taking time
        out of her busy day to listen to you; try not to insult her by accident.
*   Your Coworker understands and appreciates abstract principles, but she
    prefers their exposition be made concrete with explanations of what they
    require in practice, followed by examples and (if applicable) exceptions.
    *   *An abstract principle:* Gameplay is physically based.
    *   *An immediate consequence:* Gameplay outcomes, rather than resting on
        arbitrary numbers that seem right to the developers (e.g., damage,
        armor, movement speed), come from mathematical equations which endeavor
        to model reality. (Those equations' parameters may be tweaked to make
        the game more fun.)
    *   *A concrete example:* Under the intended model, if you add a new sword
        to the game, you don't have to worry about what bespoke damage values
        would "make sense" for it while also being "balanced"; the engine should
        use the sword's physical qualities (together with those of the wielder,
        motion, contact, etc.) to determine how many
        [joules](https://en.wikipedia.org/wiki/Joule) the sword imparts when
        swung into a foe, then convert *that* number into damage.
    *   *An exception:* Explicitly supernatural phenomena can break the laws of
        physics.
*   Your Coworker is a charitable listener and will receive your words in good
    faith. In turn, Your Coworker (like anyone else) appreciates when good faith
    is reciprocated; admit your limitations and uncertainties upfront, and do
    not be timid about them, but declare them matter-of-factly, and you will be
    accorded respect for it.
*   Your Coworker does not have any relevant domain knowledge.
    *   However, Your Coworker is intellectually curious. For any particular
        subject that threatens to lead you down a rabbit hole of follow-up
        questions, Your Coworker will accept a brief treatment with
        [hyperlinks](https://en.wikipedia.org/wiki/Hyperlink) to more dedicated
        references.
        *   A technical page can assume the reader knows how to code in
            [Rust](https://rust-lang.org/) if necessary[^4] so long as said page
            articulates the assumption beforehand.
    *   To quote Wikipedia's
        [Manual of Style](https://en.wikipedia.org/wiki/Wikipedia:Manual_of_Style#Technical_language):
        > Some topics are necessarily technical; however,
        > [editors should seek to write articles accessible to the greatest possible number of readers](https://en.wikipedia.org/wiki/Wikipedia:Make_technical_articles_understandable).
        > Minimize the use of [jargon](https://en.wikipedia.org/wiki/Jargon),
        > and adequately explain its meaning when it is used.… When the
        > concepts underlying the jargon used in an article are too complex to
        > explain
        > concisely in a parenthetical,
        > [write one level down](https://en.wikipedia.org/wiki/Wikipedia:Make_technical_articles_understandable#Write_one_level_down).
        > For example, consider adding a brief background section with
        > [\[hyperlinks\]](https://en.wikipedia.org/wiki/Hyperlink) pointing to
        > articles with a fuller treatment of the prerequisite material.
        *   Your Coworker can read an ideal Wikipedia article on a technical
            subject and enjoy it.
*   Your Coworker cannot tell whether something you're describing is *design
    intent* or *implemented behavior*—or *actual history*, *folklore*,
    *interpretation*, *speculation*, or *invention*—unless you tell her.
    <!-- TODO(wiki-truth-labels):
    Link the canonical status and provenance conventions here once they exist.
    -->
    *   Your Coworker should understand the assumptions underlying your project
        in case she ends up wanting to contribute.
*   Your Coworker is not a gamer. She knows what a video game is, but she has
    never played one. She may want to play yours.

As we said at the beginning, we write this wiki for *prospective* players among
others. A bit of a meta-design principle: even though
*Fabelgeist* is set in 16th-century Germany, punishingly difficult, and written
almost entirely in
[Shakespearean English](https://en.wikipedia.org/wiki/Early_Modern_English),
we expect it will have such broad appeal that Your Coworker will want to play
it. Everyone loves
[*Lord of the Rings*](https://www.tolkienestate.com/writing/the-lord-of-the-rings/).

## Prose references

*   [*Darklands* manual](https://cdn.steamstatic.com/steam/apps/327930/manuals/Manual.pdf)
    *   Wherein Arnold Hendrick (RIP) effortlessly explains 15th-century Germany
        to Your Coworker. Very entertaining read from page 51 onward.
*   [*Shooter: Majestic Revelations*](https://rpgwatch.com/news/deus-ex-36471.html)
    (early *Deus Ex* design doc)
    *   Wherein Warren Spector[^6] candidly lays out his design principles to
        His Coworkers. Very entertaining read throughout.

In the '90s, no one really knew what a video game was "supposed" to be, nor did
they assume much about their audience beyond a baseline level of intelligence
and imagination, but they were extremely optimistic about the places the novel
technologies of their time could take them, and that optimism comes through
abundantly in how they write about their projects—even *after* hard technical
limits have forced them to ship with only a quarter of the features they'd
hoped for at the outset.

Truth be told, though we have some guesses, none of us really know what
*Fabelgeist* or its audience will end up being either, but we're highly
optimistic on both counts, and frankly, rather few of the technological limits
of the '90s are factors for us now. Another design meta-principle: *Fabelgeist*
will renew and fulfill the promises of the '90s. Read the above references to
take in their confidence, then exercise that confidence brutally in your
writing.

<!-- TODO(design-90s-ambition):
Move the full “renew and fulfill the promises of the ’90s” principle to
design/principles.md. Retain here only its consequence for wiki prose.
-->

## Conventions

By default, we follow
[Wikipedia's Manual of Style](https://en.wikipedia.org/wiki/Wikipedia:Manual_of_Style)
for prose and
[Google's Markdown style guide](https://google.github.io/styleguide/docguide/style.html)
for source-formatting conventions. Where this page contradicts those sources,
this page takes precedence.

In particular, unlike Wikipedia, the *Fabelgeist* Wiki uses contractions,
instructional and opinionated prose, rhetorical questions, and inline external
links. Other rules specific to Wikipedia, MediaWiki, or Gitiles do not apply
either; at the time of writing, this repository uses
[mdBook](https://rust-lang.github.io/mdBook/) for Markdown publishing, so
[mdBook's Markdown conventions](https://rust-lang.github.io/mdBook/format/markdown.html)
reign. Thus, against Google's advice, we will make liberal use of relative
links, since mdBook strongly encourages them. Also, unlike Google, we use
`<!-- toc -->` rather than `[TOC]`.

The repository provides tools to enforce some of our source-formatting
conventions. If you are working from a local checkout, run
`just wiki-format path/to/page.md` on the
[command line](https://en.wikipedia.org/wiki/Command-line_interface) to wrap
its prose at 80 columns, per Google's recommendation, and `just wiki-check` to
test the tooling, verify generated navigation and documentation (e.g.
`SUMMARY.md`), check links, and ensure the book builds. To-do: automate this for
Your Coworker in our
[CI](https://en.wikipedia.org/wiki/Continuous_integration).

## See also

*   [*Steering the Craft*](https://www.ursulakleguin.com/steering-the-craft),
    Ursula K. Le Guin
*   ["If you let AI do your writing, I will come to your house and kill you"](https://samkriss.substack.com/p/if-you-let-ai-do-your-writing-i-will),
    Sam Kriss[^7]

[^1]: It is possible, whether by choice or—somewhat likelier—on account of
    age, that you, the reader, have never been in the workforce, in which case I
    expect this analogy will fall flat. I began using the Internet when I was
    four, so I certainly can see myself in your shoes. I leave this in
    regardless, not to dissuade you from contributing, but to suggest
    the general level of maturity we expect of our contributors. I didn't feel
    confident telling my friends on *Toontown Online* I was eight until they
    were convinced, from the way I wrote alone, I was *twice* that! (The game
    didn't have voice chat.)
[^2]: Something like Wikipedia or your average mathematical textbook benefits
    from a neutral writing tone because its intent is mostly to convey
    established and objective truths. However, most of what we do *here* is
    highly speculative and experimental in nature; a more personal tone helps
    suggest a measure of
    [epistemic humility](https://en.wikipedia.org/wiki/Epistemic_humility).
    Frankly, it also makes for a more engaging read; check out
    [*The Survival of the Pagan Gods*](https://press.princeton.edu/books/paperback/9780691029887/the-survival-of-the-pagan-gods),
    Seznec (1940), which—despite predating
    [V-Day](https://en.wikipedia.org/wiki/Victory_in_Europe_Day)—is much more
    layman-accessible than nearly anything its field (academic art history) has
    produced since the 1960s.
[^3]: I said "idealized."
[^4]: Rust knowledge should not be necessary for all technical pages;
    high-level algorithms, for instance, can be explained with
    [pseudocode](https://en.wikipedia.org/wiki/Pseudocode) or prose alone.
[^5]: In *Fabelgeist*, contributing and playing are tightly coupled—designing
    clothes, art, or houses for your friends are all in-game activities—but
    strictly speaking, they *are* separate: a programmer, artist, historian,
    military tactician, chemist, biologist, bodybuilder, author, or copyeditor
    could contribute substantially to our game with an email alone.
[^6]: [Warren Spector's *team*.](https://www.chrishecker.com/images/f/fc/Wspector2-small.png)
[^7]: As Kriss notes,
    [LLMs](https://en.wikipedia.org/wiki/Large_language_model) have legitimate
    technical uses. We at *Fabelgeist* use them to assist with code,
    documentation of that code (including on this wiki), research,
    organization, consistency checks, and (because we can't afford a human)
    copyediting. These are legitimate technical uses. Creative writing,
    copywriting, game design, and setting design aren't, really, at least not
    while LLMs are bad at them. (Note that many human beings are also bad at
    them. *Fabelgeist* is anti-slop, manmade or otherwise.)
