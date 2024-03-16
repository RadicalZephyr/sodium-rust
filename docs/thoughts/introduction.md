# Getting Started

## Should you use Sodium?

Sodium excels at expressing event driven programs, so daemons,
servers, GUIs, games, etc. are all a great fit for Sodium. Batch
processing programs that take some data from one source, process it
and then output it to some sink later aren't as good a fit for
Sodium. Traditional threaded programming is better at expressing these
sorts of programs, and if that's what you're building you probably
won't get much benefit from using Sodium. It might even be _more_ of a
headache to use Sodium than if you didn't.


## What do I do first?

Create a flowchart. Seriously. If you don't understand how data flows
around your program, then you won't understand how to write it in
Sodium.

Start with the edges. Where does data come into your program? Where
does it flow out into the world beyond? These are your I/O boundaries,
and Sodium is entirely and exclusively concerned with expressing what
happens in the fleeting moments between when data arrives and your
program responds.

For now, don't concern yourself with the details of where this data is
coming from, or the mechanics of how to read or write it. Just think
about the abstract moments when your program receives some signal, and
then does something in response.  The incoming edges are the _events_
that your program reacts to; in Sodium we represent these as
`Streams`. The outgoing edges are what I like to think of as the
_commands_ that your system issues. A more common nomenclature for
them in the Computer Science literature is to call them "effects".

Then, starting from one of the incoming edges, ask yourself what the
shape of the data coming from this source is. For a lot of common I/O
sources like files or sockets, the input is typically going to look
like an `std::io::Result<Vec<u8>>`. Since Sodium (currently) requires
all data sent into it to be `Clone`, a more efficient type to use
would be `Bytes` from the [bytes crate], since this is effectively an
owned reference counted byte slice `&[u8]`. So our input type would be
`std::io::Result<Bytes>`.

For now let's not worry about the error cases, let's focus on the
happy path. But we don't want to forget about error handling
completely, so we'll split this `Stream<Result<T, E>>` into two
streams, `(Stream<T>, Stream<E>)`. This is what we call "lifting" the
type into `Sodium`. Typically in a threaded Rust program, when you
have a `Result`, you'll do something to branch your code based off of
that `Result`, either an `if` or a `match` statement. Or maybe you'll
just throw that error to the surrounding context with `?`, which is
really just a hidden `match` where you `return Err()`.

In Sodium though, `Streams` are basically a code object corresponding
to a thread of execution. If we would normally branch on a `Result` in
a thread, then we can represent that in Sodium by creating two objects
from, one that represents the thread of execution following each
branch.

So, now that we've deferred handling that nasty `Err`, we are left
with just a `Bytes`. At this point you should have some functional
code to [parse][parse-dont-validate] your data into a usable data
structure. Parsing often results in it's own possible errors (because
what if someone sends you garbage data?), so we probably have another
`Result` and maybe an `Option`. We can split on these as well, and
create new `Streams` to represent each of these possibilities.

[bytes crate]: https://docs.rs/bytes/latest/bytes/
[parse-dont-validate]: https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/

#### Aside

For simplicity with Sodium today it's best to make these data
structures have no lifetimes and fully own their data. Any data that
is expensive or impossible to `Clone` should be placed behind an
`Arc`. No lock is needed inside it since we aren't going to ever be
mutating this data (this is functional programming! **No cheating!**).

### Continuing

Now that you have a data structure that you've received from some
outside source, you can make decisions about how to respond to this
event. What do you do with it? Does the answer to that depend on some
state in the program? Make a `Cell` containing that data and don't
think about how it got there. Just, conjure it from thin air and leave
those details to a future version of you, a smarter version of
you. Then call `Stream::snapshot` to capture the value of that `Cell`
at the time that your `Stream` fires.

When you need to change the shape of that data, `map` it,


# Conceptual Understanding

Sodium programs look _very_ different from other programs in the same
language. But they look a lot like programs written in Re-frame.



# Process

blargh. this is so hard, wtf, why am I doing this to myself? what kind
of masochist would sign up for writing documentation for a library
they didn't even write? I want this project to succeed, and it needs
so much love to get there. I feel like I'm trying to bail out a boat
that has already sunk. The original captain has rowed ashore and
tossed me his hat on his way.
