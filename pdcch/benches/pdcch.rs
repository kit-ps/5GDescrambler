use bit_types::llr_vec;
use bit_types::llr_vec::LLRVector;
use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion, Throughput};
use pdcch::decoder::{pdcch_decode, pdcch_decode_shared};
use pdcch::encoder::{pdcch_encode, pdcch_encode_shared};
use rand::RngExt;
use std::hint::black_box;
use std::time::Duration;

fn pdcch(c: &mut Criterion) {
    // 1. UE Search space
    // PDCCH encode
    let mut group = c.benchmark_group("pdcch_encode_ue");
    test_pdcch_encode::<108>(&mut group);
    test_pdcch_encode::<216>(&mut group);
    test_pdcch_encode::<432>(&mut group);
    test_pdcch_encode::<864>(&mut group);
    test_pdcch_encode::<1728>(&mut group);
    drop(group);

    // PDCCH decode
    let mut group = c.benchmark_group("pdcch_decode_ue");
    test_pdcch_decode::<true, 108>(&mut group);
    test_pdcch_decode::<true, 216>(&mut group);
    test_pdcch_decode::<true, 432>(&mut group);
    test_pdcch_decode::<true, 864>(&mut group);
    test_pdcch_decode::<true, 1728>(&mut group);
    drop(group);

    // PDCCH decode \wo error correction
    let mut group = c.benchmark_group("pdcch_decode_ue_wo_err_corr");
    test_pdcch_decode::<false, 108>(&mut group);
    test_pdcch_decode::<false, 216>(&mut group);
    test_pdcch_decode::<false, 432>(&mut group);
    test_pdcch_decode::<false, 864>(&mut group);
    test_pdcch_decode::<false, 1728>(&mut group);
    drop(group);

    // 2. Shared search space
    // PDCCH encode
    let mut group = c.benchmark_group("pdcch_encode_shared");
    test_pdcch_encode_shared::<108>(&mut group);
    test_pdcch_encode_shared::<216>(&mut group);
    test_pdcch_encode_shared::<432>(&mut group);
    test_pdcch_encode_shared::<864>(&mut group);
    test_pdcch_encode_shared::<1728>(&mut group);
    drop(group);

    // PDCCH decode
    let mut group = c.benchmark_group("pdcch_decode_shared");
    test_pdcch_decode_shared::<true, 108>(&mut group);
    test_pdcch_decode_shared::<true, 216>(&mut group);
    test_pdcch_decode_shared::<true, 432>(&mut group);
    test_pdcch_decode_shared::<true, 864>(&mut group);
    test_pdcch_decode_shared::<true, 1728>(&mut group);
    drop(group);

    // PDCCH decode \wo error correction
    let mut group = c.benchmark_group("pdcch_decode_shared_wo_err_corr");
    test_pdcch_decode_shared::<false, 108>(&mut group);
    test_pdcch_decode_shared::<false, 216>(&mut group);
    test_pdcch_decode_shared::<false, 432>(&mut group);
    test_pdcch_decode_shared::<false, 864>(&mut group);
    test_pdcch_decode_shared::<false, 1728>(&mut group);
    drop(group);
}

fn test_pdcch_encode<const E: usize>(c: &mut BenchmarkGroup<WallTime>) {
    const A: usize = 12;

    let mut data = [0; A];
    let mut r = rand::rng();
    for i in 0..A {
        data[i] = r.random_range(0..2);
    }
    let rnti = rand::rng().random();
    let nid = rand::rng().random();
    let matrices = &*pdcch::MATRICES;
    c.throughput(Throughput::Bits(E as u64));
    c.bench_function(format!("E = {E}").as_str(), |b| {
        b.iter(|| black_box(pdcch_encode(matrices, &data, rnti, nid, E)))
    });
}

fn test_pdcch_decode<const ENABLE_ERROR_CORRECTION: bool, const E: usize>(c: &mut BenchmarkGroup<WallTime>) {
    const A: usize = 12;

    let mut data = [0; A];
    let mut r = rand::rng();
    for i in 0..A {
        data[i] = r.random_range(0..2);
    }
    let rnti = rand::rng().random();
    let nid = rand::rng().random();
    let matrices = &*pdcch::MATRICES;
    let dec_arr = llr_vec::from_bytes(&pdcch_encode(matrices, &data, rnti, nid, E));
    let dec = LLRVector::from_data(&dec_arr);
    c.throughput(Throughput::Bits(E as u64));
    c.bench_function(format!("E = {E}").as_str(), |b| {
        b.iter(|| {
            black_box(pdcch_decode::<ENABLE_ERROR_CORRECTION, E>(matrices, &dec, 36))
        })
    });
}

fn test_pdcch_encode_shared<const E: usize>(c: &mut BenchmarkGroup<WallTime>) {
    const A: usize = 12;

    let mut data = [0; A];
    let mut r = rand::rng();
    for i in 0..A {
        data[i] = r.random_range(0..2);
    }
    let rnti = rand::rng().random();
    let nid = rand::rng().random();
    let matrices = &*pdcch::MATRICES;
    c.throughput(Throughput::Bits(E as u64));
    c.bench_function(format!("E = {E}").as_str(), |b| {
        b.iter(|| black_box(pdcch_encode_shared(matrices, &data, rnti, nid, E)))
    });
}

fn test_pdcch_decode_shared<const ENABLE_ERROR_CORRECTION: bool, const E: usize>(c: &mut BenchmarkGroup<WallTime>) {
    const A: usize = 12;

    let mut data = [0; A];
    let mut r = rand::rng();
    for i in 0..A {
        data[i] = r.random_range(0..2);
    }
    let rnti = rand::rng().random();
    let nid = rand::rng().random();
    let matrices = &*pdcch::MATRICES;
    let dec_arr = llr_vec::from_bytes(&pdcch_encode_shared(matrices, &data, rnti, nid, E));
    let dec = LLRVector::from_data(&dec_arr);
    c.throughput(Throughput::Bits(E as u64));
    c.bench_function(format!("E = {E}").as_str(), |b| {
        b.iter(|| {
            black_box(pdcch_decode_shared::<ENABLE_ERROR_CORRECTION, E>(matrices, &dec, 36))
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = pdcch
}
criterion_main!(benches);
