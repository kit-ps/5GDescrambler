use pdcch::encoder::pdcch_encode;
use rand::RngExt;
use std::hint::black_box;
use std::time::Duration;
use criterion::{criterion_group, BenchmarkGroup, Criterion, Throughput, criterion_main, BatchSize};
use criterion::measurement::WallTime;
use phys::pipeline::decode::ca_scl_polar_pdcch_known_params::decode_pdcch_polar;

fn pdcch_polar(c: &mut Criterion) {
    let mut group = c.benchmark_group("pdcch_decode_polar");
    test_pdcch_decode::<108, 2>(&mut group);
    test_pdcch_decode::<216, 2>(&mut group);
    test_pdcch_decode::<432, 2>(&mut group);
    test_pdcch_decode::<864, 2>(&mut group);
    test_pdcch_decode::<1728, 2>(&mut group);

    test_pdcch_decode::<108, 4>(&mut group);
    test_pdcch_decode::<216, 4>(&mut group);
    test_pdcch_decode::<432, 4>(&mut group);
    test_pdcch_decode::<864, 4>(&mut group);
    test_pdcch_decode::<1728, 4>(&mut group);

    test_pdcch_decode::<108, 8>(&mut group);
    test_pdcch_decode::<216, 8>(&mut group);
    test_pdcch_decode::<432, 8>(&mut group);
    test_pdcch_decode::<864, 8>(&mut group);
    test_pdcch_decode::<1728, 8>(&mut group);

    test_pdcch_decode::<108, 16>(&mut group);
    test_pdcch_decode::<216, 16>(&mut group);
    test_pdcch_decode::<432, 16>(&mut group);
    test_pdcch_decode::<864, 16>(&mut group);
    test_pdcch_decode::<1728, 16>(&mut group);

    test_pdcch_decode::<108, 32>(&mut group);
    test_pdcch_decode::<216, 32>(&mut group);
    test_pdcch_decode::<432, 32>(&mut group);
    test_pdcch_decode::<864, 32>(&mut group);
    test_pdcch_decode::<1728, 32>(&mut group);

    test_pdcch_decode::<108, 64>(&mut group);
    test_pdcch_decode::<216, 64>(&mut group);
    test_pdcch_decode::<432, 64>(&mut group);
    test_pdcch_decode::<864, 64>(&mut group);
    test_pdcch_decode::<1728, 64>(&mut group);

    test_pdcch_decode::<108, 128>(&mut group);
    test_pdcch_decode::<216, 128>(&mut group);
    test_pdcch_decode::<432, 128>(&mut group);
    test_pdcch_decode::<864, 128>(&mut group);
    test_pdcch_decode::<1728, 128>(&mut group);
}

fn test_pdcch_decode<const E: usize, const L: usize>(c: &mut BenchmarkGroup<WallTime>) {
    const A: usize = 12;
    const K: usize = A + 24;

    let mut data = [0; A];
    let mut r = rand::rng();
    for i in 0..A {
        data[i] = r.random_range(0..2);
    }
    let rnti = rand::rng().random();
    let nid = rand::rng().random();
    let matrices = &*pdcch::MATRICES;
    let llrs = pdcch_encode(matrices, &data, rnti, nid, E).iter().map(|x| *x as f32 * -2f32 + 1f32).collect::<Vec<_>>();
    c.throughput(Throughput::Bits(E as u64));
    c.bench_function(format!("E = {E}, L = {L}").as_str(), |b| {
        b.iter_batched(
            || llrs.clone(),
            |mut llrs| {
                black_box(decode_pdcch_polar::<L>(llrs.as_mut_slice(), K, rnti, nid, false))
            },
            BatchSize::SmallInput
        )
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = pdcch_polar
}
criterion_main!(benches);
