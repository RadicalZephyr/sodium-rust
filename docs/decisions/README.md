# Architecture Decision Records

An ADR records a decision that shaped the library: what we chose, what we
turned down, and why. The point is to save the next person -- usually us, a
year later -- from re-litigating a question that was already settled, or from
re-settling it without knowing what the first answer cost.

Not every change needs one. Write an ADR when the reasoning is the valuable
part: a trade-off between two workable designs, a constraint that is not
obvious from the code, a decision whose alternatives will look tempting again
later.

## Files

One record per file, `NNNN-kebab-case-title.md`, numbered from `0001` in the
order they are written. Numbers are never reused. A number is an identifier, not
a chronology -- it is what supersession references point at, which is why it
survives a retitle.

Suggested sections, though the record should follow the argument rather than the
template:

- **Status** -- omitted entirely when the decision is made and built. See below.
- **Date** -- YYYY-MM-DD, when the record was written.
- **Context** -- the forces in play, including anything measured.
- **Decision** -- what we are doing.
- **Consequences** -- what this buys and what it costs, including the parts we
  are not happy about.
- **Alternatives considered** -- and why each was turned down.

Where discussion produced a conflict or a trade-off, write it into the section it
belongs to rather than into a changelog at the bottom. The disagreement is
usually the most useful thing in the document.

## Records are living documents

A record is **edited to stay current**, not frozen on acceptance. Read
`docs/decisions/` as the current state of the decisions made in this repository.

Git is the log; the document is the projection. Every revision is kept and dated
already, and `git log -p` on a record is there for anyone who wants the diff, so
nothing is lost by keeping the file readable as it stands today.

What makes that safe is that new information arrives as a **dated addition,
marked as arriving after the decision** -- never as a silent revision of the
original reasoning:

> **2027-02-14 (after the decision):** re-running the benchmark on the 1.99
> toolchain closes the gap to 4%, which weakens but does not reverse the
> argument below.

Accretion, not overwriting. Without that discipline a mutable record drifts
toward what we now think we thought, and stops being evidence of a commitment
made under specific information.

[`0001-recording-important-decisions.md`](0001-recording-important-decisions.md)
argues for all of this.

## Status

The `Status` line records **deviation from the expected lifecycle**. Made and
built is the terminal state and carries no line at all; every status that exists
names a way the record is not in that state.

| Status | Meaning | Set when |
| --- | --- | --- |
| `Draft` | still being argued | while the pull request is open |
| `Accepted YYYY-MM-DD -- not yet implemented` | decided, not yet built | a commit before the record's PR merges |
| *(no status line)* | decided and built | the line is deleted by the last PR implementing it |
| `Superseded by NNNN` | replaced | a commit before the superseding record merges |
| `Deprecated -- see [section]` | withdrawn, not replaced | when the reversal is written into the record |

Two consequences worth knowing:

- The absence of a status is never reached by forgetting. A record merges
  carrying its accepted date, and the line is *deleted* later, deliberately, by
  the pull request that finishes the work.
- The accepted date is not decoration. A record that sits unimplemented is the
  hot air a decisions directory is prone to, and dating it makes it age in
  public.

`Deprecated` links to the section of its own record that explains the reversal.
That is the difference from `Superseded`: superseded points outward to the
successor, deprecated points inward to a dated addendum.

## Editing versus superseding

> Edit the record when the decision it documents is still the decision. Write a
> new one when someone following the old record would now be doing the wrong
> thing.

The mechanical form of the same test, which is usually quicker to apply:

> Can the change be written as a dated addition? It is an edit. Does it require
> deleting a claim someone might have acted on? The old claim deserves to
> survive as its own record.

New data supporting the existing choice, a widened scope, a clarity rewrite: all
edits. A reversal, or a decision that stands while its mechanism changes out from
under it: a new record. Deliberately a judgement about a reader rather than
something countable -- diff size measures effort and gets this wrong in both
directions.

## Dating claims that will not age well

Anything in a record that is true *as of* rather than true: benchmark numbers,
costs, quoted compiler output, toolchain behaviour, third-party capabilities.
Say when it was measured, and against what.

This is not a separate convention from the version stamp on quoted rustc output
below -- that is this rule's first and strictest instance.

## Research

Code that produces concrete data used in the argumentation of an ADR --
benchmarks, memory measurements, probes into rustc behaviour -- **must** be
committed somewhere a reader can run it. A number quoted in an ADR should be
re-derivable by anyone with a checkout; if the experiment only ever existed in
a scratch buffer, the ADR is asserting rather than arguing.

Which of the two homes it gets is decided by what it depends on:

| The experiment needs | It lives in |
| --- | --- |
| `sodium-rust` | the `adr-research` crate in [`research/`](research/), as a binary named after the record |
| only rustc and std | a [Rust Playground](https://play.rust-lang.org) share link, with the source in the ADR |

The split enforces itself in one direction: a Playground cannot depend on this
crate, so anything that fits in one is necessarily a minimal reproduction. It
is also the only route open to a compile-time experiment -- a case that must
*fail* to compile cannot be a research binary, because a binary that does not
compile breaks the workspace build.

See [`research/README.md`](research/README.md) for the crate, including how an
experiment is retired.

### Playground experiments carry three things, not one

A share link is live, not frozen. Its `version=stable` is a *channel*, not a
version, so it re-runs against whatever stable is current on the day someone
clicks it -- the wording of a diagnostic will drift out from under the record,
and the same code may eventually compile clean. So a Playground experiment is
recorded as three artifacts, each doing one job:

1. **The share link** -- live convenience, one click to a running compiler.
2. **The source, in a code block** -- the frozen record. Not a backup against
   the gist being deleted; it is what the ADR actually argued from.
3. **The rustc version the quoted output came from** -- e.g. *rustc 1.98.1
   (2026-09-01), edition 2021*, stamped beside the output.

The version stamp is **required, and reviewers check for it**. Without it a
reader who clicks through to different output cannot tell whether rustc moved
or the record was always wrong. With it, that disagreement is itself the signal
that a superseding record is due.

Research is evidence, not a test. An ADR arguing for different internals should
not bring tests with it -- they would be written against the structure the
record exists to replace. The exception is an ADR whose argument is that this
implementation diverges from Sodium's denotational semantics: that one gets a
deliberately failing test in `src/tests.rs`, marked
`#[ignore = "ADR-NNNN: ..."]`. See
[`CONTRIBUTING.md`](../../CONTRIBUTING.md#tests-and-research-are-not-the-same-thing).
