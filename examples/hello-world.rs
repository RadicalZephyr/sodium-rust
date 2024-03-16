use sodium_rust as na;

fn main() {
    let ctx = na::SodiumCtx::new();

    // Start an explicit transaction for creating the whole Sodium
    // system at once.
    let tx = na::Transaction::new(&ctx);

    // Create stream sinks so you can provide input to your Sodium program.
    let hello_sink = ctx.new_stream_sink();

    // Get the Stream connected to your sink to react to input events.
    let hello_s = hello_sink.stream();

    // Constant data can be stored in a `Cell`.
    let name_c = ctx.new_cell(String::from("World"));


    // Combine input events and program state to produce new output.
    let hello_name_s = hello_s.snapshot(&name_c, |greeting: &String, name: &String| {
        format!("{greeting} {name}!")
    });

    // End the creation transaction by dropping it.
    drop(tx);

    // Register listeners to allow your Sodium program to produce output.
    let mut listeners = Vec::new();
    listeners.push(hello_name_s.listen(|message: &String| println!("{message}")));

    // Send an "input" event because this is hello world.
    hello_sink.send(String::from("Hello"));

    // Clean up all of our listeners now that we no longer need them
    // so the whole system can be garbage collected.
    for l in listeners {
        l.unlisten();
    }
}
