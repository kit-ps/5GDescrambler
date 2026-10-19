use pdcch_macros::matrix_struct;
use std::sync::LazyLock;

pub mod decoder;
pub mod encoder;
mod lgs_error_correction;
mod rank_nullity;

const BYTES: &[u8] = include_bytes!(concat!("../gen/", env!("TARGET_ARCH"), ".bin"));

matrix_struct!();

pub static MATRICES: LazyLock<&'static Matrices> = LazyLock::new(|| {
    // Safety: This is safe, because we only need an uninitialized area in heap to write to in the next steps.
    // Size is checked at the end of this method.
    let buf = Box::leak(unsafe { Box::new_uninit().assume_init() });
    let bytes = bytemuck::bytes_of_mut(buf);
    let size = zstd::bulk::decompress_to_buffer(BYTES, &mut *bytes).expect("Decompression of binary matrices failed");
    assert_eq!(size, size_of::<Matrices>(), "Incorrect size of decoded matrices struct");
    buf
});