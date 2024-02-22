use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, AxisScale, PlotConfiguration};

use sodium_rust::{SodiumCtx, StreamSink, Stream};

fn stream(c: &mut Criterion) {
    let mut group = c.benchmark_group("Graph Size");
    group.throughput(criterion::Throughput::Bytes(1));
    let plot_config = PlotConfiguration::default()
        .summary_scale(AxisScale::Logarithmic);
    group.plot_config(plot_config);

    for e in 0_u32..6 {
        let ctx = SodiumCtx::new();
        let sink: StreamSink<u8> = ctx.new_stream_sink();

        let mut s = sink.stream();
        let nodes = 2_usize.pow(e);
        for _ in 1..nodes {
            s = s.map(|x: &u8| black_box(*x));
        }

        let mut v = Vec::with_capacity(1024);
        let l = s.listen(move |x: &u8| v.push(black_box(*x)));

        group.bench_with_input(BenchmarkId::new("Linear", nodes), &sink, |b, sink| {
            b.iter(|| sink.send(black_box(0)));
        });
        l.unlisten();
        drop(ctx);

        let ctx = SodiumCtx::new();
        let sink: StreamSink<u8> = ctx.new_stream_sink();

        let stream = sink.stream();

        let mut listeners = Vec::with_capacity(nodes.div_euclid(2));
        match e {
            0 => {
                let mut v = Vec::with_capacity(1024);
                listeners.push(stream.listen(move |x: &u8| v.push(black_box(*x))));
            }
            1.. => {
                // Add 1 node if e > 0
                let stream = stream.map(|x: &u8| black_box(*x));

                // Only create a tree of depth e-1, because a perfeect
                // binary tree has `2^(n+1) - 1` nodes, where `n` is
                // the depth. With the +1 node from above, and a depth
                // of `n = e - 1`, the number of nodes is `2^((e-1)+1) = 2^e`
                let leaves = branch(e-1, stream);

                for leaf in leaves {
                    let mut v = Vec::with_capacity(1024);
                    listeners.push(leaf.listen(move |x: &u8| v.push(black_box(*x))));
                }

            }
        }

        assert_eq!(nodes.div_euclid(2), listeners.len());
        group.bench_with_input(BenchmarkId::new("Full Tree", nodes), &sink, |b, sink| {
            b.iter(|| sink.send(black_box(0)));
        });
        for l in listeners {
            l.unlisten();
        }
        drop(ctx);
    }
    group.finish();
}

fn branch(n: u32, s: Stream<u8>) -> Vec<Stream<u8>> {
    if n == 0 {
        vec![s]
    } else {
        let left = s.map(|x: &u8| black_box(*x));
        let right = s.map(|x: &u8| black_box(*x));
        let mut leaves = branch(n-1, left);
        leaves.extend(branch(n-1, right));
        leaves
    }
}

criterion_group!(benches, stream);
criterion_main!(benches);
