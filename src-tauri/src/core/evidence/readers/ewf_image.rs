//! Pure Rust EWF adapter, shared by desktop, CLI and mount workflows.
//!
//! The reader is the `ewf` crate from SecurityRonin/ewf-forensic. Keep this
//! serialized acquisition contract stable for existing cases and the frontend.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EwfAcquiryInfo {
    pub case_number: Option<String>,
    pub evidence_number: Option<String>,
    pub examiner_name: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub acquiry_date: Option<String>,
    pub system_date: Option<String>,
    pub acquiry_operating_system: Option<String>,
    pub acquiry_software_version: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub stored_md5: Option<String>,
    pub stored_sha1: Option<String>,
    pub format: Option<String>,
    pub media_type: Option<String>,
    pub compression: Option<String>,
    pub segment_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EwfSegmentFileSnapshot {
    pub path: PathBuf,
    pub len: u64,
    pub modified_unix_ms: Option<i64>,
    pub created_unix_ms: Option<i64>,
}

#[derive(Debug)]
pub struct EwfHandle {
    reader: ewf::EwfReader,
    acquiry: EwfAcquiryInfo,
    segments_on_disk_bytes: u64,
    segment_file_snapshots: Vec<EwfSegmentFileSnapshot>,
}

impl EwfHandle {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let paths = ewf::discover_segments(path).context("discover EWF segments")?;
        let mut segment_file_snapshots = Vec::with_capacity(paths.len());
        let mut segments_on_disk_bytes = 0u64;
        for path in &paths {
            let metadata = path
                .metadata()
                .with_context(|| format!("stat EWF segment {}", path.display()))?;
            segments_on_disk_bytes = segments_on_disk_bytes
                .checked_add(metadata.len())
                .context("EWF segment sizes overflow")?;
            segment_file_snapshots.push(EwfSegmentFileSnapshot {
                path: path.clone(),
                len: metadata.len(),
                modified_unix_ms: metadata.modified().ok().and_then(unix_ms),
                created_unix_ms: metadata.created().ok().and_then(unix_ms),
            });
        }
        // Bound chunk-table memory for multi-terabyte v1 images. v2 currently
        // uses the upstream eager table, with the same positioned-read API.
        let reader = ewf::EwfReader::open_segments_lazy(&paths, 128, 8)
            .context("open EWF with ewf-forensic reader")?;
        let sector_size = reader.bytes_per_sector();
        anyhow::ensure!(
            (128..=65536).contains(&sector_size) && sector_size.is_power_of_two(),
            "invalid EWF sector size: {sector_size}"
        );
        let addressable = reader
            .chunk_size()
            .checked_mul(reader.chunk_count() as u64)
            .context("EWF addressable size overflow")?;
        anyhow::ensure!(
            addressable >= reader.total_size(),
            "incomplete EWF segment set: table covers {addressable} of {} bytes",
            reader.total_size()
        );
        let metadata = reader.metadata();
        let hashes = reader.stored_hashes();
        let acquiry = EwfAcquiryInfo {
            case_number: metadata.case_number.clone(),
            evidence_number: metadata.evidence_number.clone(),
            examiner_name: metadata.examiner.clone(),
            description: metadata.description.clone(),
            notes: metadata.notes.clone(),
            acquiry_date: metadata.acquiry_date.clone(),
            system_date: metadata.system_date.clone(),
            acquiry_operating_system: metadata.os_version.clone(),
            acquiry_software_version: metadata.acquiry_software.clone(),
            model: metadata.model.clone(),
            serial_number: metadata.serial_number.clone(),
            stored_md5: hashes.md5.map(hex::encode),
            stored_sha1: hashes.sha1.map(hex::encode),
            format: Some(
                if reader.is_v2() {
                    "EWF2 (EnCase 7+)"
                } else {
                    "EWF1"
                }
                .into(),
            ),
            media_type: metadata.media_type.map(|kind| match kind {
                0 => "可移动介质".into(),
                1 => "固定磁盘".into(),
                3 => "光盘".into(),
                0x0e => "逻辑文件".into(),
                0x10 => "内存 (RAM)".into(),
                other => format!("0x{other:02X}"),
            }),
            compression: metadata.compression.clone(),
            segment_count: Some(u32::try_from(paths.len()).context("too many EWF segments")?),
        };
        Ok(Self {
            reader,
            acquiry,
            segments_on_disk_bytes,
            segment_file_snapshots,
        })
    }

    pub fn media_size(&self) -> u64 {
        self.reader.total_size()
    }
    pub fn bytes_per_sector(&self) -> usize {
        self.reader.bytes_per_sector() as usize
    }
    pub fn acquiry_info(&self) -> &EwfAcquiryInfo {
        &self.acquiry
    }
    pub fn segments_on_disk_bytes(&self) -> Option<u64> {
        Some(self.segments_on_disk_bytes)
    }
    pub fn segment_file_snapshots(&self) -> &[EwfSegmentFileSnapshot] {
        &self.segment_file_snapshots
    }

    pub fn read_at(&self, offset: u64, buffer: &mut [u8]) -> anyhow::Result<usize> {
        self.reader
            .read_at(buffer, offset)
            .with_context(|| format!("EWF read at byte {offset}"))
    }
}

fn unix_ms(time: std::time::SystemTime) -> Option<i64> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
}

#[cfg(test)]
#[path = "ewf_image_tests.rs"]
mod tests;
