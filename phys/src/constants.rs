/// The maximum supported RX ports for use by channel equalization.
pub const MAX_RX_PORTS: usize = 64;

/// This factor is used to distinguish SSS correlations. Maximum value a SSS correlation may reach
/// is 127.
pub const SSS_THRESHOLD_FACTOR: f32 = 70f32;

/// Threshold for DMRS correlation to consider a PDCCH candidate as valid. 1.0 is the maximum value of a perfect match.
pub const DMRS_CORRELATION_THRESHOLD: f32 = 0.6f32;

/// Chunk size for SIMD operations on Complex32 values
pub const CF32_SIMD_CHUNK_SIZE: usize = 512;

/// Acceptable STD angle for active group detection.
/// Values are interpreted in QPSK quadrants and thus divided by four:
/// - STD of PI means 45 degrees (PI/4)
/// - STD of FRAC_PI_3 thus corresponds to an angle of PI/12 (15 degrees)
pub const ACCEPTABLE_STD_ANGLE_ACTIVE_GROUP: f32 = std::f32::consts::FRAC_PI_3;

/// The maximum amount of standard deviation allowed compared to the average amplitude for
/// active group detection.
/// 1 divided by this factor corresponds to minimum SNR.
pub const ACCEPTABLE_STD_AMPLITUDE_ACTIVE_GROUP: f32 = 0.3;

/// Denotes the acceptable amount of RE that may be inactive per group.
/// This should be below 5 so the risk of classifying CSI or arbitrary DMRS (with only half of the
/// RE active per group) is minimized.
pub const ACCEPTABLE_INACTIVE_PER_GROUP: f32 = 3f32;

/// The maximum amount of combinations to try per aggregation layer and symbol.
/// Limits the effort to spend on computations (it is not worth brute-forcing a candidate that
/// might occur isolated later in time).
/// Indexes correspond to AL 1, 2, 4, 8 and 16 respectively.
pub const MAX_COMBINATIONS_PER_SYMBOL_AND_AL: [usize; 5] = [256, 256, 128, 64, 32];

/// The maximum acceptable amount of bits of uncertainty to correct on aggregation layer 1.
/// Increasing this will lead to more false positives. Decreasing leads to less success rate.
/// If set to 0, will disable error correction on aggregation layer 1 entirely (not recommended).
pub const MAX_ERRORS_AND_FREE_AL_1: usize = 2;

/// If enabled and a block to search matches a specific aggregation layer, this aggregation layer will
/// be searched first.
pub const TRY_FITTING_AL_FIRST: bool = true;