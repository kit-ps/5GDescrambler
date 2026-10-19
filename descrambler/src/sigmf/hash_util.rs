use sha2::{Digest, Sha512};

pub fn hasher_to_str(hasher: Sha512) -> String {
    let result = hasher.finalize();

    let hex_string: String = result
        .iter()
        .map(|b| format!("{:02x}", *b))
        .collect();

    hex_string
}

pub fn hash_sha512(data: &[u8]) -> std::io::Result<String> {
    let mut hasher = Sha512::new();
    hasher.update(data);
    Ok(hasher_to_str(hasher))
}