//! Compare full, streamed and ranged NTFS reads, optionally against oracle bytes.
use anyhow::{ensure, Context};
use lovelycarve_lib::core::{evidence::image::DiskImage, filesystem::volumes::ntfs::NtfsVolume};
use sha2::{Digest, Sha256};

fn main() -> anyhow::Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        arguments.len() >= 3,
        "usage: probe_ntfs_stream <image> <partition-id> <record> [oracle-file]"
    );
    let image = DiskImage::open(&arguments[0])?;
    let partition_id: u8 = arguments[1].parse()?;
    let partition = image
        .partitions
        .iter()
        .find(|partition| partition.id == partition_id)
        .context("NTFS partition not found")?;
    let volume = NtfsVolume::open(&image, partition)?;
    let record: u64 = arguments[2].parse()?;
    let stat = volume.file_stat(record, "probe")?;
    ensure!(stat.size <= 16 * 1024 * 1024, "probe file exceeds 16 MiB");
    let bytes = volume.read_file(record, stat.size as usize)?;
    ensure!(bytes.len() as u64 == stat.size, "short full read");
    let expected_hash = Sha256::digest(&bytes);
    let mut stream_hash = Sha256::new();
    let count = volume.stream_file(record, 77777, |chunk| {
        stream_hash.update(chunk);
        Ok(())
    })?;
    ensure!(
        count == stat.size && stream_hash.finalize() == expected_hash,
        "streamed bytes differ"
    );
    let mut ranges = 0;
    for offset in [
        0,
        1,
        15,
        4090,
        65530,
        bytes.len() / 2,
        bytes.len().saturating_sub(17),
        bytes.len(),
    ] {
        let offset = offset.min(bytes.len());
        let end = (offset + 70001).min(bytes.len());
        ensure!(
            volume.read_range(record, offset as u64, 70001)? == bytes[offset..end],
            "range differs at {offset}"
        );
        ranges += 1;
    }
    if let Some(path) = arguments.get(3) {
        let oracle = std::fs::read(path)?;
        ensure!(bytes == oracle, "NTFS bytes differ from independent oracle");
    }
    println!(
        "record={record} size={} sha256={} ranges={ranges} oracle={}",
        stat.size,
        hex::encode(expected_hash),
        arguments.get(3).is_some()
    );
    Ok(())
}
