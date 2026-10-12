//! Verifies that `locate_active_metadata_region` finds the active text
//! region on a real PV without falling back to the text scan.
//!
//! Usage: cargo run --example probe_lvm_active <image>

use lovelycarve_lib::core::evidence::image::DiskImage;
use lovelycarve_lib::core::evidence::partitions::lvm2::metadata::locate_active_metadata_region;
use lovelycarve_lib::core::evidence::partitions::PartitionKind;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: probe_lvm_active <image>"))?;
    let image = DiskImage::open(&path)?;
    for partition in &image.info().partitions {
        if partition.kind != PartitionKind::Lvm {
            continue;
        }
        let off = partition.start_lba * image.sector_size as u64;
        let len = (partition.length_lba * image.sector_size as u64).min(4 * 1024 * 1024) as usize;
        let probe = image.read_range(off, len)?;
        match locate_active_metadata_region(&probe) {
            Some((start, end)) => {
                println!(
                    "partition {} binary-locator: hit at probe[{}..{}] ({} bytes)",
                    partition.id,
                    start,
                    end,
                    end - start
                );
                let preview: String = probe[start..end]
                    .iter()
                    .take(180)
                    .map(|&b| {
                        if (0x20..=0x7e).contains(&b) || b == b'\n' || b == b'\t' {
                            b as char
                        } else {
                            '.'
                        }
                    })
                    .collect();
                println!("preview: {preview}");
            }
            None => {
                println!(
                    "partition {} binary-locator: MISS (text-scan fallback would run)",
                    partition.id
                );
            }
        }
    }
    Ok(())
}
