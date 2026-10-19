use serde::de::DeserializeOwned;
use std::fs::File;
use std::io;
use std::io::{BufReader, Read};
use std::path::PathBuf;

pub fn read_f32_file(path: &PathBuf) -> io::Result<Vec<f32>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::with_capacity(file.metadata()?.len() as usize);
    file.read_to_end(&mut bytes)?;

    let floats = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();

    Ok(floats)
}

pub fn read_u8_file(path: &PathBuf) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::with_capacity(file.metadata()?.len() as usize);
    file.read_to_end(&mut bytes)?;

    Ok(bytes)
}

pub fn read_json_file<T: DeserializeOwned>(path: &PathBuf) -> io::Result<T> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let data = serde_json::from_reader(reader)?;
    Ok(data)
}