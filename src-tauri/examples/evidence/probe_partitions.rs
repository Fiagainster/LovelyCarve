//! Partition-detection probe. Usage: just run-example probe_partitions <image> [--mbr]
use std::env;

use lovelycarve_lib::core::evidence::image::DiskImage;
use lovelycarve_lib::core::evidence::partitions::mbr::MbrTable;

fn main() -> anyhow::Result<()> {
    let mut arguments = env::args().skip(1);
    let path = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: probe_partitions <image> [--mbr]"))?;
    let show_mbr = match arguments.next().as_deref() {
        None => false,
        Some("--mbr") => true,
        Some(argument) => anyhow::bail!("unknown argument: {argument}"),
    };
    anyhow::ensure!(arguments.next().is_none(), "too many arguments");
    let image = DiskImage::open(&path)?;
    let info = image.info();
    println!("image: {}", info.path);
    println!("format: {:?}", info.source_format);
    println!(
        "size:   {} bytes ({} sectors @ {})",
        info.size_bytes,
        info.size_bytes / info.sector_size as u64,
        info.sector_size
    );
    println!("partitions: {}", info.partitions.len());
    for p in &info.partitions {
        let start = p.start_lba * info.sector_size as u64;
        let len = p.length_lba * info.sector_size as u64;
        println!(
            "  #{id:<3} kind={kind:<14} start_lba={slba:<12} length_lba={llba:<12} offset=0x{off:x} size={size} label={label:?}",
            id = p.id,
            kind = format!("{:?}", p.kind),
            slba = p.start_lba,
            llba = p.length_lba,
            off = start,
            size = human(len),
            label = p.label
        );
    }
    if show_mbr {
        if let Some(table) = MbrTable::read(&image, info.sector_size)? {
            for record in &table.records {
                println!(
                    "MBR/EBR lba={} disk_signature=0x{:08X} signed={}",
                    record.record_lba, record.disk_signature, record.has_signature
                );
                for (slot, entry) in record.entries.iter().enumerate() {
                    println!("  slot={slot} entry_offset={:?} type=0x{:02X} boot=0x{:02X} start_chs={:?} end_chs={:?} relative_lba={} length_lba={}", record.entry_offset(slot, table.sector_size), entry.type_byte, entry.boot_indicator, entry.start_chs(), entry.end_chs(), entry.start_lba, entry.length_lba);
                }
            }
            for warning in &table.warnings {
                println!("warning: {warning}");
            }
        } else {
            println!("no MBR table");
        }
    }
    Ok(())
}

fn human(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.2} {}", UNITS[unit])
}
