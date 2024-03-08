# Design Paralysis

Two difficulties I've encountered so far.

 - Switching paradigms
 - No intuitive design yet


Writing FRP is different than imperative or even functional
programming. You're setting up a web of code that will respond to
plucks externally once it's complete, but while building code I keep
finding myself at this mental impasse where I have an idea of what I
want to do, but no idea how to do it.

This often leads me to just sitting and staring at the code in an
unhelpful way.

In general, Sodium code is pretty easy to refactor, especially with a
language like Rust that has strong refactoring tools already. So while
planning is good, if you get stuck it's probably better to try
something and move forward than to just sit and try to think it out.
