//! LovelyCarve additions for geometry and acquisition metadata compatibility.
use crate::{EwfError, EwfMetadata, Result};

pub(crate) fn utf16_text(raw: &[u8]) -> String {
    String::from_utf16_lossy(
        &raw.chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>(),
    )
    .trim_start_matches('\u{feff}')
    .to_owned()
}

pub(crate) fn device_metadata(raw: &[u8], metadata: &mut EwfMetadata) -> Result<()> {
    let text = utf16_text(raw);
    let lines: Vec<_> = text.lines().collect();
    if let (Some(names), Some(values)) = (lines.get(2), lines.get(3)) {
        for (name, value) in names.split('\t').zip(values.split('\t')) {
            if value.is_empty() {
                continue;
            }
            match name {
                "b" => {
                    metadata.bytes_per_sector = Some(
                        value
                            .parse()
                            .map_err(|_| EwfError::Parse("invalid EWF2 sector size".into()))?,
                    )
                }
                "md" => metadata.model = Some(value.into()),
                "sn" => metadata.serial_number = Some(value.into()),
                _ => {}
            }
        }
    }
    Ok(())
}
