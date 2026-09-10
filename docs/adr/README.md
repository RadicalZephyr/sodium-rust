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
benchmarks, memory measurements, probes into rustc behaviour -- **must** live
in the `adr-research` workspace crate in [`research/`](research/), as a binary
named after the ADR it serves. See [`research/src/lib.rs`](research/src/lib.rs)
for the convention.

A number quoted in an ADR should be re-derivable by anyone with a checkout. If
the experiment only ever existed in a scratch buffer, the ADR is asserting
rather than arguing.
