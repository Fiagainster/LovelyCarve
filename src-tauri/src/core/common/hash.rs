//! Hash computation helpers. Streaming for large inputs, one-shot for small.

use md5::{Digest as _, Md5};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use sm3::Sm3;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn sha512_hex(bytes: &[u8]) -> String {
    let mut h = Sha512::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn md5_hex(bytes: &[u8]) -> String {
    let mut h = Md5::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Short "abbreviated" hash used in UI:  `9f2a…c4e1`
pub fn abbreviate(hex_str: &str) -> String {
    if hex_str.len() < 10 {
        hex_str.to_string()
    } else {
        format!("{}…{}", &hex_str[..4], &hex_str[hex_str.len() - 4..])
    }
}

/// Algorithms exposed by the source-image hash command. Kept in lockstep
/// with the frontend `HashAlgo` literal union so a typo in either side
/// fails to deserialize at the Tauri boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HashAlgo {
    Md5,
    Sha1,
    Sha256,
    Sm3,
}

/// Streaming hasher that drives several algorithms from a single byte
/// stream. Each chunk is fed once; the digests are finalised together via
/// [`MultiHasher::finalize`]. Used by `image_hash_compute` so a 100 GB
/// image is decoded and read exactly once for all selected algorithms.
pub struct MultiHasher {
    md5: Option<Md5>,
    sha1: Option<Sha1>,
    sha256: Option<Sha256>,
    sm3: Option<Sm3>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MultiHashOutput {
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub sm3: Option<String>,
}

impl MultiHasher {
    pub fn new(algos: &[HashAlgo]) -> Self {
        let mut hasher = Self {
            md5: None,
            sha1: None,
            sha256: None,
            sm3: None,
        };
        for algo in algos {
            match algo {
                HashAlgo::Md5 => hasher.md5 = Some(Md5::new()),
                HashAlgo::Sha1 => hasher.sha1 = Some(Sha1::new()),
                HashAlgo::Sha256 => hasher.sha256 = Some(Sha256::new()),
                HashAlgo::Sm3 => hasher.sm3 = Some(Sm3::new()),
            }
        }
        hasher
    }

    pub fn update(&mut self, chunk: &[u8]) {
        if let Some(h) = self.md5.as_mut() {
            h.update(chunk);
        }
        if let Some(h) = self.sha1.as_mut() {
            h.update(chunk);
        }
        if let Some(h) = self.sha256.as_mut() {
            h.update(chunk);
        }
        if let Some(h) = self.sm3.as_mut() {
            h.update(chunk);
        }
    }

    pub fn finalize(self) -> MultiHashOutput {
        MultiHashOutput {
            md5: self.md5.map(|h| hex::encode(h.finalize())),
            sha1: self.sha1.map(|h| hex::encode(h.finalize())),
            sha256: self.sha256.map(|h| hex::encode(h.finalize())),
            sm3: self.sm3.map(|h| hex::encode(h.finalize())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_algorithms_match_known_abc_digests() {
        let expected = (
            "900150983cd24fb0d6963f7d28e17f72",
            "a9993e364706816aba3e25717850c26c9cd0d89d",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0",
        );
        let mut hasher = MultiHasher::new(&[
            HashAlgo::Md5,
            HashAlgo::Sha1,
            HashAlgo::Sha256,
            HashAlgo::Sm3,
        ]);
        hasher.update(b"a");
        hasher.update(b"bc");
        let output = hasher.finalize();
        assert_eq!(
            (
                output.md5.as_deref(),
                output.sha1.as_deref(),
                output.sha256.as_deref(),
                output.sm3.as_deref(),
            ),
            (
                Some(expected.0),
                Some(expected.1),
                Some(expected.2),
                Some(expected.3),
            )
        );
        assert_eq!(
            (md5_hex(b"abc"), sha1_hex(b"abc"), sha256_hex(b"abc")),
            (
                expected.0.to_owned(),
                expected.1.to_owned(),
                expected.2.to_owned()
            )
        );
        assert_eq!(sha512_hex(b"abc"), "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f");
    }

    #[test]
    fn multi_hasher_matches_single_pass() {
        let data = b"the quick brown fox jumps over the lazy dog";
        let mut multi = MultiHasher::new(&[
            HashAlgo::Md5,
            HashAlgo::Sha1,
            HashAlgo::Sha256,
            HashAlgo::Sm3,
        ]);
        multi.update(&data[..15]);
        multi.update(&data[15..]);
        let out = multi.finalize();
        assert_eq!(out.md5.as_deref(), Some(md5_hex(data).as_str()));
        assert_eq!(out.sha1.as_deref(), Some(sha1_hex(data).as_str()));
        assert_eq!(out.sha256.as_deref(), Some(sha256_hex(data).as_str()));
        // SM3 sanity: feed the same bytes one-shot and compare.
        let mut sm3 = Sm3::new();
        sm3.update(data);
        let sm3_expected = hex::encode(sm3.finalize());
        assert_eq!(out.sm3.as_deref(), Some(sm3_expected.as_str()));
    }

    #[test]
    fn multi_hasher_skips_unselected() {
        let mut multi = MultiHasher::new(&[HashAlgo::Sm3]);
        multi.update(b"hello world");
        let out = multi.finalize();
        assert!(out.md5.is_none());
        assert!(out.sha1.is_none());
        assert!(out.sha256.is_none());
        assert!(out.sm3.is_some());
    }
}
