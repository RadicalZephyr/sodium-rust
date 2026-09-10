# 0001 -- Recording important decisions

<details>
<summary><strong>Status:</strong> Draft</summary>

| Date | Transition |
| --- | --- |
| 2026-09-10 | Drafted |

</details>

*This record is its own first worked example. Because the rules it describes are
implemented by the same pull request, the pre-merge commit logs `Accepted` and
`Implemented` on the same date -- the one case where those two transitions
collapse, and a demonstration that the log can say so plainly where a single
status field could not.*

## Context

This repository had nowhere to put reasoning. The code carries a lot of it in
comments, `docs/internals/insights.md` holds what one of us reconstructed by
tracing the internals, and `CHANGELOG.md` records what changed without saying
why. None of that survives the question "we already settled this a year ago --
what did the first answer cost?"

Two things forced the question now. The first is that sodium-rust is one port
in the [Sodium](https://github.com/SodiumFRP) family: the API's shape and its
semantics are mandated by Sodium's denotational semantics and are not ours to
renegotiate. So practically every decision we will make here is about the
internal implementation, or about how those fixed semantics should be spelled
in Rust specifically. Both are things a decision *intends* to change, which
turns out to constrain how the reasoning can be recorded.

The second is that we had just finished a change -- moving the public
combinators from `IsLambda1..6` bounds to bare `Fn`/`FnMut` -- that produced a
pile of prototyping artifacts with nowhere to live. Some of them ended up in
the test suite, where they have been failing for reasons unrelated to this
library ever since. That is the concrete failure this record is trying not to
repeat.

## Decision

Decisions live in `docs/decisions/`, one record per file, and they are **living
documents**: a record is edited to stay current rather than frozen at the
moment it was written.

The rules themselves live in [`README.md`](README.md) beside this file, and in
[`CONTRIBUTING.md`](../../CONTRIBUTING.md) for contributors. This record holds
the argument, what we turned down, and what we are still unsure about. Keeping
the two apart is deliberate: a record that restates its own rulebook goes stale
the first time the rulebook changes.

## Why living documents rather than immutable records

The usual ADR practice freezes a record on acceptance and supersedes it with a
new one. We are not doing that, and the reason is sharper than "immutability is
a computer-science habit applied to a human process."

An append-only log is half of a pattern. Event-sourced systems work because
they keep the log *and* materialise a view from it. Immutable ADR practice
keeps the log and then tells you to read the log -- so reconstructing the
current state means walking a supersession chain, which in a repository this
size nobody will ever do.

The synthesis was already available: **git is the log, the document is the
projection.** Making records mutable does not discard history, it finally
builds the view that makes the history usable. Git keeps every revision, dated,
and `git log -p` on a record is there for anyone who wants the diff.

The strongest argument for immutability is not preservation but cost --
write-once records need no maintenance. That is an accounting trick. A frozen
record that has gone stale still costs; the cost just moves to read time and is
charged to every reader instead of once to an author. For a directory we want
to function as documentation, read cost is the one that matters.

## Dated strata are what make mutability safe

The real hazard of editable records is not lost history, it is **hindsight
contamination**: once a record can be revised, it drifts toward "what we now
think we thought," and stops being evidence of a commitment made under specific
information. Git does not protect against this, because nobody reads git.

So new information enters a record as a **dated addition, marked as arriving
after the decision** -- the practice [the ADR community calls
timestamps](https://github.com/architecture-decision-record/architecture-decision-record).
That single constraint converts "living document" from overwriting into
accretion, which preserves exactly what immutability was protecting while
leaving the file readable as current state.

The same discipline applies to any claim that will not age well: benchmark
numbers, toolchain behaviour, quoted compiler output. We arrived at a special
case of this rule before we found the general one -- see *Evidence* below.

## The status field

A record's status is a **dated transition log**, collapsed at the top of the
file, with the current state in the summary line. `README.md` has the shape and
the set of transitions.

The reasoning that got us here is worth keeping, because we tried a simpler thing
first. The original design was a single `Status` line that recorded *deviation
from the expected lifecycle*: made-and-built was the terminal state and carried
no line at all, on the argument that a field answering "what is unusual about
this record?" does not need to say "nothing" -- the same shape as a passing test
carrying no attribute where `#[ignore = "ADR-NNNN: ..."]` marks the one that
fails.

That argument holds, and it is still why we do not want a bare `Implemented`
line on forty records. But it defended an asymmetry rather than removing one, and
it left a real cost: absence cannot distinguish "done" from "the author forgot."

The log dissolves both problems instead of trading between them. `Implemented`
becomes a row rather than an absence, so the special case stops existing; and
because every row is dated, the line is never noise -- `Accepted 2026-09-10` then
`Implemented 2026-11-03` is a fact about how this project actually moves.

It is also the more consistent application of our own principle. We adopted
living documents because forcing a reader to reconstruct state from git is the
failure we were trying to escape, and then put state transitions in exactly that
place. The gap between `Accepted` and `Implemented` is the hot-air metric a
decisions directory most needs: a decision the code never honoured shows up as a
row that never arrived, visible in the file rather than derivable from
`git log`.

## Editing versus superseding

The rule we needed was when to revise a record and when to write a new one that
supersedes it. Our first instinct was to ground it in diff size -- substantially
rewriting a record means writing a new one instead.

That does not survive contact. Changing "we will use X" to "we will not use X"
is four characters and unambiguously a new decision; rewriting an entire record
for clarity without touching a conclusion is a total diff and unambiguously not.
Diff size measures effort, and correlates with the thing we care about in
neither direction. It is also, ironically, the *more* rigid option: counting
lines looks precise while being wrong.

The rule is grounded in consequence to a reader instead:

> Edit the record when the decision it documents is still the decision. Write a
> new one when someone following the old record would now be doing the wrong
> thing.

With a mechanical companion that makes this rule and the dated-strata rule
enforce each other:

> Can the change be written as a dated addition? It is an edit. Does it require
> deleting a claim someone might have acted on? The old claim deserves to
> survive as its own record.

`Superseded` and `Deprecated` differ in where the reasoning lives, not whether
it exists. **Superseded points outward** -- the argument is in the successor
record. **Deprecated points inward** -- the argument is a dated addendum in the
record itself, and the status line links to it.

We looked for a case where a record would be deprecated with no reasoning worth
capturing anywhere, and could not construct one. Even the degenerate case --
accepting a decision and then simply not doing it -- has a reason attached, and
that reason is a dated addendum rather than a whole new record, because nothing
was built for anyone to have acted on. A useful intuition, though not a rule:
`Deprecated` is mostly what happens to a record that died in the not-yet-
implemented state, and `Superseded` is what happens to one that shipped.

## Evidence

A number quoted in a record has to be re-derivable by a reader, or the record is
asserting rather than arguing. Experiments are routed by what they depend on --
`sodium-rust` puts them in the `adr-research` crate, rustc-and-std-only puts
them in a Rust Playground link recorded in the document. `README.md` has the
routing table and the retirement rule.

Two things about that arrangement are decisions rather than mechanics.

**The Playground constraint is a feature.** A Playground cannot depend on this
crate, so anything that fits in one is necessarily a minimal reproduction. It is
also the only route open to a compile-time experiment, because a binary that
does not compile breaks the workspace build. Verified against trybuild 1.0.121:
a `compile_fail` case with no `.stderr` file does not assert "failed to compile,
reason unimportant" -- it writes to `wip/` and panics the run (`src/run.rs`, the
`created_wip` branch). Pinning the exact diagnostic wording is not incidental to
that technique, it *is* the technique.

**A share link is live, not frozen**, which is why a Playground record carries
three artifacts rather than one. `version=stable` in the URL is a channel, not a
version, so the link re-runs against whatever stable is current when it is
clicked. The link is convenience, the code block is the frozen record, and the
rustc version that produced the quoted output is what makes the record
falsifiable later.

That last requirement came from being bitten. On 2026-09-10 a `compile_fail`
case in `tests/ui/` failed locally while CI was green: rustc 1.94.1 (2026-03-25)
emits ``expected a `FnMut(&i32)` closure`` where rustc 1.98.1 (2026-09-01) emits
``expected an``. The obvious fix -- re-blessing with `TRYBUILD=overwrite` --
would have committed the older wording and turned CI red, in a diff consisting
of one article. Without a version stamp, a reader cannot tell whether the
compiler moved or the record was always wrong.

## Tests are not evidence

A decision arguing for different internals does **not** bring tests with it. Such
a test is written against the structure the record exists to replace, so landing
the change means rewriting it: it guarded nothing and only enlarged the diff.

The exception is a record whose argument is that our operational semantics
diverge from Sodium's denotational semantics. That is a bug report rather than a
design preference, and the test written for it stays correct after the fix
because it is written against semantics we do not control. It goes in
`src/tests.rs` marked `#[ignore = "ADR-NNNN: ..."]`, written as the behaviour the
library *ought* to have, so it fails.

`#[ignore]` was chosen over a quarantine module or a `should_panic` because of a
property confirmed on rustc 1.98.1 (2026-09-01): the reason string prints on
every ordinary test run, not only under `--ignored`.

```rust
#[test]
#[ignore = "ADR-0007: switch_c ought to take the inner cell's value in the same transaction"]
fn switch_c_simultaneous() {
    assert_eq!(1, 2);
}
```

```text
test switch_c_simultaneous ... ignored, ADR-0007: switch_c ought to take the inner cell's value in the same transaction
```

So a known gap advertises itself in normal output while CI stays green, and
landing the fix deletes one attribute line rather than rewriting a test.

*This experiment wants a Playground share link alongside the source, per the
rule above. It does not have one yet: the environment this record was drafted in
blocks `play.rust-lang.org` at the network policy. Added when someone with
access can produce it.*

## Alternatives considered

**Immutable records with a supersession chain.** The conventional practice, and
what we have used on other repositories. Rejected because it optimises for the
writer at the reader's expense, and because in practice it produced records
permanently marked "draft" -- a symptom of having no criterion for when a draft
ends. Making acceptance a merge gate dissolves that.

**Diff size as the edit-versus-supersede criterion.** Rejected above. It
measures effort rather than consequence and fails in both directions.

**Collapsing the status field entirely**, with implementation state inferred
from the code. Rejected: it reproduces the failure it was meant to fix, making a
reader reconstruct current state from somewhere else. It also assumed a reader
looking at a pull request on GitHub, where the surrounding interface supplies
the context. A record is frequently read from a local checkout at a commit --
especially in this repository, where the reason to check out is to run the
experiments a record cites -- and there it must speak for itself.

**A single `Status` line with no marker for the implemented state.** Our own
first design, superseded within the same pull request by the transition log. The
argument for it is in *The status field* above; what it could not do was
distinguish a finished record from a forgotten one, and it kept state
transitions in git after we had just finished arguing that git is where state
goes to be ignored.

**A plain `## History` section at the bottom of the record** rather than a
collapsed block at the top. Rejected because it puts the current state at the
opposite end of the document from where a reader starts, and duplicates it
between a top-line status and a bottom-row log. The collapsed block keeps one
source of truth and puts it first.

## Consequences

Living records converge on **explanation**. A record that is continuously
updated becomes a description of why the architecture is the way it is, which is
[Diataxis](https://diataxis.fr/) explanation in all but name. That is the
mechanism by which this directory becomes useful documentation rather than an
archive, not drift to be corrected.

The shape of the directory is a design signal. Few records edited often means a
small number of load-bearing decisions being refined -- healthy. Many records
edited rarely means the decisions are independent, and the directory is an
archive that will want an index. **Many records edited often is the warning**:
decisions are entangled, so the code's seams do not match the decisions' seams.
It looks like health from inside, because everything is current and active. For
this library the first place to look would be the public wrapper / `impl_`
split, the one boundary nearly every decision touches on both sides.

`adr-research` should trend toward empty. Every experiment in it has a scheduled
exit -- deleted once the internals it measured are gone, or promoted to
`benches/` or `src/tests.rs` once it turns out to answer a live question.
Accumulation means something was miscategorised.

`CLAUDE.md` currently holds a substantial architecture section, which is
explanation wearing agent-instruction clothing. That is an accepted interim:
`docs/` has no home for it yet. When the documentation is reorganised, that
section moves out and `CLAUDE.md` keeps the operational half -- commands,
conventions, and the traps.

## Open questions

**How `docs/explanation/` and `docs/decisions/` divide.** Because this project
is an existing codebase being adapted -- and a fairly direct port of the C++
implementation -- the reasoning behind its current shape may not exist in this
repository's history at all. An explanation document here would often have to
say "this is what the code does, and we do not know why it came to be this way."
Deriving that history from the code alone would be fabrication.

So explanation and decisions will overlap, from opposite directions: explanation
describes a present nobody can account for, while decisions accumulate the
richer history from here forward. When a decision is later made about something
an explanation document covers, some of that document is context for the record
-- but probably not all of it, and the record should not swallow the whole thing.

We are deliberately not settling this in advance. `docs/internals/insights.md`
is the case in miniature: undigested observations with no decision attached,
which may graduate into a record, or into explanation, or legitimately stay
where they are. That last option is the disanalogy with `adr-research` -- an
experiment has no valid resting state, an observation does. We will have a
better feel for the boundary with a real case in front of us.
