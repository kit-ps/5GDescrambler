pub mod hash_util;

use crate::sigmf::hash_util::hasher_to_str;
use hash_util::hash_sha512;
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;

pub const SIGMF_VERSION: &str = "1.2.5";
pub const SIGMF_COLLECTION_EXT: &str = "sigmf-collection";
pub const SIGMF_META_EXT: &str = "sigmf-meta";
pub const SIGMF_DATA_EXT: &str = "sigmf-data";
pub const ACCEPTED_DATA_TYPE: &str = "cf32_le";

#[derive(Serialize, Deserialize)]
pub struct SigMfCollection {
    pub collection: CollectionConfig,
}

#[derive(Serialize, Deserialize)]
pub struct SigMfMeta {
    pub global: GlobalMetaConfig,
    pub captures: Vec<CaptureMetaConfig>,
}

#[derive(Serialize, Deserialize)]
pub struct CollectionConfig {
    #[serde(rename = "core:version")]
    pub version: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "core:description")]
    pub description: Option<String>,

    /// Needs to have at least one stream
    #[serde(rename = "core:streams")]
    pub streams: Vec<CollectionStreamConfig>,
}

#[derive(Serialize, Deserialize)]
pub struct CollectionStreamConfig {
    pub hash: String,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct GlobalMetaConfig {
    /// Required to be cf32_le
    #[serde(rename = "core:datatype")]
    pub data_type: String,

    #[serde(rename = "core:sample_rate")]
    pub sample_rate: u64,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "core:description")]
    pub description: Option<String>,

    /// Required to be 1
    #[serde(skip_serializing_if = "is_1", default = "default_1")]
    #[serde(rename = "core:num_channels")]
    pub num_channels: usize,

    /// Required to be false
    #[serde(skip_serializing_if = "is_false", default = "default_false")]
    #[serde(rename = "metadata_only")]
    pub metadata_only: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "core:recorder")]
    pub recorder: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "core:sha512")]
    pub sha512: Option<String>,

    #[serde(rename = "core:version")]
    pub version: String,
}

#[derive(Serialize, Deserialize)]
pub struct CaptureMetaConfig {
    /// Required to be 0
    #[serde(rename = "core:sample_start")]
    pub sample_start: u64,

    #[serde(rename = "core:frequency")]
    pub frequency: u64,
}

#[allow(unused)]
pub struct InputFilesConfig<P: AsRef<Path>> {
    pub capture_frequency: u64,
    pub sample_rate: u64,
    pub channels: Vec<P>,
}

const fn default_1() -> usize {
    1
}

const fn is_1(b: &usize) -> bool {
    *b == 1
}

const fn default_false() -> bool {
    false
}

const fn is_false(b: &bool) -> bool {
    !*b
}

impl SigMfCollection {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.collection.streams.is_empty() {
            return Err("No streams provided");
        }
        for stream in &self.collection.streams {
            if stream.hash.is_empty() {
                return Err("Hash for a stream is empty");
            }
            if stream.name.is_empty() {
                return Err("Name for a stream is empty");
            }
        }

        Ok(())
    }
}

impl SigMfMeta {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.global.data_type.as_str().eq(ACCEPTED_DATA_TYPE) {
            return Err("Only cf32_le supported as data type");
        }
        if self.global.num_channels != 1 {
            return Err("Only single channel SigMF meta files supported");
        }
        if self.global.metadata_only {
            return Err("SigMF meta file indicated no data present");
        }

        if self.captures.len() != 1 {
            return Err("Only exactly one capture in SIGMF meta file is supported");
        }
        for capture in &self.captures {
            if capture.sample_start != 0 {
                return Err("Capture contained in SIGMF meta file that is not starting at 0. This is unsupported.");
            }
        }

        Ok(())
    }
}

pub fn create_sigmf<P: AsRef<Path> + ?Sized, Q: AsRef<Path>>(path: &P, files: &[Q], hashes: Vec<Sha512>, collection_name: &str, capture_frequency: u64, sample_rate: u64) -> std::io::Result<impl AsRef<Path>> {
    let mut streams = Vec::with_capacity(files.len());
    for (file, hasher) in files.iter().zip(hashes) {
        let hash = hasher_to_str(hasher);
        let meta = SigMfMeta {
            global: GlobalMetaConfig {
                data_type: ACCEPTED_DATA_TYPE.to_string(),
                sample_rate,
                description: Some("5G cell capture".to_string()),
                num_channels: 1,
                metadata_only: false,
                recorder: Some("5GDescrambler".to_string()),
                sha512: Some(hash),
                version: SIGMF_VERSION.to_string(),
            },
            captures: vec![CaptureMetaConfig {
                sample_start: 0,
                frequency: capture_frequency,
            }],
        };
        let stem = file.as_ref().file_stem().ok_or(std::io::Error::other("File should have a name"))?;
        let meta_path = path.as_ref().join(stem).with_added_extension(SIGMF_META_EXT);
        let json = serde_json::to_string_pretty(&meta)?;
        let data = json.as_bytes();
        streams.push(CollectionStreamConfig {
            hash: hash_sha512(data)?,
            name: stem.to_str().ok_or(std::io::Error::other("File should have a valid name"))?.to_string(),
        });
        std::fs::write(&meta_path, data)?;
    }

    let collection = SigMfCollection {
        collection: CollectionConfig {
            version: SIGMF_VERSION.to_string(),
            description: Some("5G cell capture".to_string()),
            streams,
        },
    };

    let json = serde_json::to_string_pretty(&collection)?;
    let col_path = path.as_ref().join(collection_name).with_added_extension(SIGMF_COLLECTION_EXT);
    std::fs::write(&col_path, json.as_bytes())?;

    Ok(col_path)
}

pub fn load_sigmf<P: AsRef<Path>>(path: &P) -> std::io::Result<InputFilesConfig<impl AsRef<Path>>> {
    let path = path.as_ref().with_extension(SIGMF_COLLECTION_EXT);
    let collection: SigMfCollection = serde_json::from_reader(File::open(&path)?)?;
    collection.validate().map_err(|e| std::io::Error::other(e))?;
    let dir = path.parent().ok_or(std::io::Error::other("Collection should have a parent directory"))?;

    let mut channels = Vec::with_capacity(collection.collection.streams.len());
    let mut capture_frequency = 0u64;
    let mut sample_rate = 0u64;
    for stream in &collection.collection.streams {
        let data: SigMfMeta = serde_json::from_reader(File::open(dir.join(&stream.name).with_added_extension(SIGMF_META_EXT))?)?;
        data.validate().map_err(|e| std::io::Error::other(e))?;
        if sample_rate == 0 {
            capture_frequency = data.captures[0].frequency;
            sample_rate = data.global.sample_rate;
        }
        if sample_rate != data.global.sample_rate {
            return Err(std::io::Error::other("Sample rate mismatch between channels!"));
        }
        if capture_frequency != data.captures[0].frequency {
            return Err(std::io::Error::other("Capture frequency mismatch between channels!"));
        }
        let spath = dir.join(&stream.name).with_added_extension(SIGMF_DATA_EXT);
        if !spath.exists() {
            return Err(std::io::Error::new(ErrorKind::NotFound, format!("{spath:?}")));
        }
        channels.push(spath);
    }

    Ok(InputFilesConfig {
        capture_frequency,
        sample_rate,
        channels,
    })
}