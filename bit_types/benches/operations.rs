use bit_types::bit_matrix::BitMatrix;
use bit_types::bit_vector::BitVector;
use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};
use rand::RngExt;
use std::hint::black_box;
use std::time::Duration;

fn operations(c: &mut Criterion) {
    // Nmax in 5G is 1024, min DCI size with crc is 48 -> valid values for N are:
    // 64, 128, 256, 512, 1024

    // Matrix * matrix
    let mut group = c.benchmark_group("mat * mat");
    test_matmul_n::<2, 108>(&mut group);
    test_matmul_n::<4, 216>(&mut group);
    test_matmul_n::<8, 432>(&mut group);
    test_matmul_n::<16, 864>(&mut group);
    test_matmul_n::<32, 1728>(&mut group);

    // Const matrix * matrix
    test_matmul_const_n::<2, 108>(&mut group);
    test_matmul_const_n::<4, 216>(&mut group);
    test_matmul_const_n::<8, 432>(&mut group);
    test_matmul_const_n::<16, 864>(&mut group);
    test_matmul_const_n::<32, 1728>(&mut group);
    drop(group);

    // Vector * matrix
    let mut group = c.benchmark_group("vec * mat");
    test_vecmatmul_n::<2, 108>(&mut group);
    test_vecmatmul_n::<4, 216>(&mut group);
    test_vecmatmul_n::<8, 432>(&mut group);
    test_vecmatmul_n::<16, 864>(&mut group);
    test_vecmatmul_n::<32, 1728>(&mut group);

    // Const vector * matrix
    test_vecmatmul_const_n::<2, 108>(&mut group);
    test_vecmatmul_const_n::<4, 216>(&mut group);
    test_vecmatmul_const_n::<8, 432>(&mut group);
    test_vecmatmul_const_n::<16, 864>(&mut group);
    test_vecmatmul_const_n::<32, 1728>(&mut group);
    drop(group);

    // Vector * vector
    let mut group = c.benchmark_group("vec * vec");
    test_vecmul_n::<2, 108>(&mut group);
    test_vecmul_n::<4, 216>(&mut group);
    test_vecmul_n::<8, 432>(&mut group);
    test_vecmul_n::<16, 864>(&mut group);
    test_vecmul_n::<32, 1728>(&mut group);

    // Const vector * vector
    test_vecmul_const_n::<2, 108>(&mut group);
    test_vecmul_const_n::<4, 216>(&mut group);
    test_vecmul_const_n::<8, 432>(&mut group);
    test_vecmul_const_n::<16, 864>(&mut group);
    test_vecmul_const_n::<32, 1728>(&mut group);
}

fn test_matmul_n<const LANES: usize, const N: usize>(c: &mut BenchmarkGroup<WallTime>) {
    let data1 = rand::rng().random::<[[u64; LANES]; N]>();
    let mat1 = BitMatrix::<LANES, LANES, N, N>::new_with_data(data1);
    let data2 = rand::rng().random::<[[u64; LANES]; N]>();
    let mat2 = BitMatrix::<LANES, LANES, N, N>::new_with_data(data2);
    c.bench_function(format!("{N}").as_str(), |b| {
        b.iter(|| black_box(mat1.mul_mat(&mat2)))
    });
}

fn test_vecmatmul_n<const LANES: usize, const N: usize>(c: &mut BenchmarkGroup<WallTime>) {
    let data1 = rand::rng().random::<[[u64; LANES]; N]>();
    let mat = BitMatrix::<LANES, LANES, N, N>::new_with_data(data1);
    let data2 = rand::rng().random::<[u64; LANES]>();
    let vec = BitVector::<LANES, N>::new_with_data(data2);
    c.bench_function(format!("{N}").as_str(), |b| {
        b.iter(|| black_box(mat.mul_vec(&vec)))
    });
}

fn test_vecmul_n<const LANES: usize, const N: usize>(c: &mut BenchmarkGroup<WallTime>) {
    let data1 = rand::rng().random::<[u64; LANES]>();
    let vec1 = BitVector::<LANES, N>::new_with_data(data1);
    let data2 = rand::rng().random::<[u64; LANES]>();
    let vec2 = BitVector::<LANES, N>::new_with_data(data2);
    c.bench_function(format!("{N}").as_str(), |b| {
        b.iter(|| black_box(vec1.mul_vec(&vec2)))
    });
}

fn test_matmul_const_n<const LANES: usize, const N: usize>(c: &mut BenchmarkGroup<WallTime>) {
    let data1 = rand::rng().random::<[[u64; LANES]; N]>();
    let mat1 = BitMatrix::<LANES, LANES, N, N>::new_with_data(data1);
    let data2 = rand::rng().random::<[[u64; LANES]; N]>();
    let mat2 = BitMatrix::<LANES, LANES, N, N>::new_with_data(data2);
    c.bench_function(format!("const {N}").as_str(), |b| {
        b.iter(|| black_box(mat1.mul_mat_const(&mat2)))
    });
}

fn test_vecmatmul_const_n<const LANES: usize, const N: usize>(c: &mut BenchmarkGroup<WallTime>) {
    let data1 = rand::rng().random::<[[u64; LANES]; N]>();
    let mat = BitMatrix::<LANES, LANES, N, N>::new_with_data(data1);
    let data2 = rand::rng().random::<[u64; LANES]>();
    let vec = BitVector::<LANES, N>::new_with_data(data2);
    c.bench_function(format!("const {N}").as_str(), |b| {
        b.iter(|| black_box(mat.mul_vec_const(&vec)))
    });
}

fn test_vecmul_const_n<const LANES: usize, const N: usize>(c: &mut BenchmarkGroup<WallTime>) {
    let data1 = rand::rng().random::<[u64; LANES]>();
    let vec1 = BitVector::<LANES, N>::new_with_data(data1);
    let data2 = rand::rng().random::<[u64; LANES]>();
    let vec2 = BitVector::<LANES, N>::new_with_data(data2);
    c.bench_function(format!("const {N}").as_str(), |b| {
        b.iter(|| black_box(vec1.mul_vec_const(&vec2)))
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = operations
}
criterion_main!(benches);
