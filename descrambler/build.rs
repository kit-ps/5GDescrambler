fn main() {
    // Warn if compiling without SDR support
    if std::env::var("CARGO_FEATURE_SDR").is_err() {
        println!("cargo:warning=Compiling without SDR support! All SDR commands will be unavailable.");
    }
}