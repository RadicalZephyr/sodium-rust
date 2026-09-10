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
order they are written. Numbers are never reused, and a superseded record is
never deleted -- it gets a `Superseded by NNNN` status and stays where it is.

Every ADR opens as a **Draft**. A draft can be edited freely, without recording
the history of its own revisions; once it is **Accepted** the record is
effectively frozen, and a change of mind is a new ADR that supersedes it.

Suggested sections, though the record should follow the argument rather than
the template:

- **Status** -- Draft, Accepted, or Superseded by NNNN.
- **Date** -- YYYY-MM-DD,
- **Context** -- the forces in play, including anything measured.
- **Decision** -- what we are doing.
- **Consequences** -- what this buys and what it costs, including the parts we
  are not happy about.
- **Alternatives considered** -- and why each was turned down.

Where discussion produced a conflict or a trade-off, write it into the section
it belongs to rather than into a changelog at the bottom. The disagreement is
usually the most useful thing in the document.

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
