//! Quick partition-detection probe. Usage: cargo run --example probe_partitions -- <image>
use std::env;

use lovelycarve_lib::core::evidence::image::DiskImage;

fn main() -> anyhow::Result<()> {
    let path = env::args().nth(1).expect("usage: probe_partitions <image>");
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
