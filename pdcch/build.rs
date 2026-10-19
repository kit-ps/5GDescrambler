use bit_types::bit_matrix::BitMatrix;
use pdcch_macros::{matrix_gen_all, matrix_struct};
use std::thread::Builder;
use std::{env, fs};
use zstd::bulk::compress;

matrix_struct!();

const STACK_SIZE: usize = 4 * 1024 * 1024 * 1024;

fn main() {
    // Validate architecture settings
    #[cfg(target_endian = "little")]
    let endian = "little";
    #[cfg(target_endian = "big")]
    let endian = "big";
    #[cfg(target_pointer_width = "32")]
    let pointer_width = 32;
    #[cfg(target_pointer_width = "64")]
    let pointer_width = 64;

    let arch_id = format!("{}-{}-{}",
                          env::var("HOST").expect("$HOST environment variable not set"),
                          pointer_width,
                          endian
    );

    let target_arch = format!("{}-{}-{}",
                              env::var("TARGET").expect("$TARGET environment variable not set"),
                              env::var("CARGO_CFG_TARGET_POINTER_WIDTH").expect("$CARGO_CFG_TARGET_POINTER_WIDTH environment variable not set"),
                              env::var("CARGO_CFG_TARGET_ENDIAN").expect("$CARGO_CFG_TARGET_ENDIAN environment variable not set"),
    );

    assert_eq!(arch_id, target_arch, "Compiler host architecture must match target architecture for matrix embedding");

    println!("cargo:rustc-env=TARGET_ARCH={}", target_arch);

    // Configure file change hooks
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=gen/{arch_id}.bin");

    // Skip generation if already generated
    if let Ok(x) = fs::exists(format!("gen/{arch_id}.bin")) && x {
        println!("cargo:warning=Build script of PDCCH crate was rerun, but skipped matrix binary generation. Regenerating matrices may take 1-2 minutes. Please remove gen/{arch_id}.bin if you intend to regenerate matrices!");
        return;
    }

    // Increase stack size so we can handle large matrices on the stack
    let child = Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || run(&arch_id))
        .expect("Failed to spawn matrix generation thread");

    child.join().expect("Failed to join matrix generation thread");
}

fn run(arch_id: &str) {
    // Generate all matrices
    let s = matrix_gen_all!();

    // Write file
    fs::create_dir_all("gen").expect("Failed to create directory for matrix binary generation");
    let bytes = bytemuck::bytes_of(&*s);
    let compress = compress(bytes, 5).expect("Failed to compress matrix binary");
    fs::write(format!("gen/{arch_id}.bin"), compress).expect("Failed to write matrix binary");
}

#[inline(always)]
fn map_decode<const LANES: usize, const VEC_SIZE: usize, const RATE_MATCHING_LIMIT: usize>(input: (usize, [usize; 32], Box<BitMatrix<LANES, LANES, RATE_MATCHING_LIMIT, RATE_MATCHING_LIMIT>>, Box<BitMatrix<LANES, LANES, VEC_SIZE, RATE_MATCHING_LIMIT>>)) -> Box<DecodeMatrix<LANES, VEC_SIZE, RATE_MATCHING_LIMIT>> {
    let (num_free, free_bits, mat, solved) = input;
    Box::new(DecodeMatrix {
        num_free,
        free_bits,
        mat: *mat,
        solved: *solved,
    })
}
