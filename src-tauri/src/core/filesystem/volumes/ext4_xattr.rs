//! Minimal Ext4 extended-attribute decoding used by filesystem adapters.
//!
//! Ext4 stores attributes either after the inode's extra fields or in the
//! block referenced by `i_file_acl`.  OverlayFS uses those attributes for
//! opaque-directory and regular-file whiteout markers, so exposing them here
//! lets higher-level virtual filesystems reproduce the mounted view without
//! asking the host kernel to mount evidence.

const EXT4_XATTR_MAGIC: u32 = 0xEA02_0000;
const ENTRY_HEADER_SIZE: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ext4Xattr {
    pub name: String,
    pub value: Vec<u8>,
}

pub fn parse_inode_xattrs(inode: &[u8], extra_isize: u16) -> Vec<Ext4Xattr> {
    let header_start = 128usize.saturating_add(extra_isize as usize);
    let Some(magic) = read_u32(inode, header_start) else {
        return Vec::new();
    };
    if magic != EXT4_XATTR_MAGIC {
        return Vec::new();
    }
    let first_entry = header_start + 4;
    parse_entries(inode, first_entry, first_entry)
}

pub fn parse_xattr_block(block: &[u8]) -> Vec<Ext4Xattr> {
    if read_u32(block, 0) != Some(EXT4_XATTR_MAGIC) {
        return Vec::new();
    }
    // `ext4_xattr_header` is 32 bytes. In an external block e_value_offs is
    // relative to the beginning of the block, unlike the in-inode form.
    parse_entries(block, 32, 0)
}

fn parse_entries(storage: &[u8], mut cursor: usize, value_base: usize) -> Vec<Ext4Xattr> {
    let mut attributes = Vec::new();
    let mut remaining_entries = 4096usize;
    while remaining_entries > 0 {
        remaining_entries -= 1;
        if storage.get(cursor..cursor + 4) == Some(&[0, 0, 0, 0]) {
            break;
        }
        let Some(header) = storage.get(cursor..cursor + ENTRY_HEADER_SIZE) else {
            break;
        };
        let name_len = header[0] as usize;
        let name_index = header[1];
        let value_offset = u16::from_le_bytes([header[2], header[3]]) as usize;
        let value_inode = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let value_size =
            u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;
        let entry_size = align4(ENTRY_HEADER_SIZE.saturating_add(name_len));
        let Some(name_bytes) =
            storage.get(cursor + ENTRY_HEADER_SIZE..cursor + ENTRY_HEADER_SIZE + name_len)
        else {
            break;
        };
        let Some(next_cursor) = cursor.checked_add(entry_size) else {
            break;
        };
        if next_cursor > storage.len() {
            break;
        }

        // EA-inode values need a second inode lookup and are not used by the
        // OverlayFS markers this decoder serves. Keep parsing subsequent
        // entries but omit a value we cannot substantiate from this buffer.
        if value_inode == 0 {
            if let Some(value_start) = value_base.checked_add(value_offset) {
                if let Some(value_end) = value_start.checked_add(value_size) {
                    if let Some(value) = storage.get(value_start..value_end) {
                        attributes.push(Ext4Xattr {
                            name: attribute_name(name_index, name_bytes),
                            value: value.to_vec(),
                        });
                    }
                }
            }
        }
        cursor = next_cursor;
    }
    attributes
}

fn attribute_name(index: u8, raw_name: &[u8]) -> String {
    let suffix = String::from_utf8_lossy(raw_name);
    match index {
        1 => format!("user.{suffix}"),
        2 => "system.posix_acl_access".to_string(),
        3 => "system.posix_acl_default".to_string(),
        4 => format!("trusted.{suffix}"),
        6 => format!("security.{suffix}"),
        7 => format!("system.{suffix}"),
        8 => "system.richacl".to_string(),
        _ => format!("index{index}.{suffix}"),
    }
}

fn align4(value: usize) -> usize {
    value.saturating_add(3) & !3
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inline_trusted_overlay_attribute() {
        let mut inode = vec![0u8; 256];
        inode[128..130].copy_from_slice(&32u16.to_le_bytes());
        inode[160..164].copy_from_slice(&EXT4_XATTR_MAGIC.to_le_bytes());
        let entry = 164usize;
        let name = b"overlay.opaque";
        inode[entry] = name.len() as u8;
        inode[entry + 1] = 4;
        inode[entry + 2..entry + 4].copy_from_slice(&76u16.to_le_bytes());
        inode[entry + 8..entry + 12].copy_from_slice(&1u32.to_le_bytes());
        inode[entry + 16..entry + 16 + name.len()].copy_from_slice(name);
        inode[240] = b'y';

        assert_eq!(
            parse_inode_xattrs(&inode, 32),
            vec![Ext4Xattr {
                name: "trusted.overlay.opaque".to_string(),
                value: vec![b'y'],
            }]
        );
    }

    #[test]
    fn parses_external_user_whiteout_attribute() {
        let mut block = vec![0u8; 4096];
        block[0..4].copy_from_slice(&EXT4_XATTR_MAGIC.to_le_bytes());
        let entry = 32usize;
        let name = b"overlay.whiteout";
        block[entry] = name.len() as u8;
        block[entry + 1] = 1;
        block[entry + 2..entry + 4].copy_from_slice(&4000u16.to_le_bytes());
        block[entry + 8..entry + 12].copy_from_slice(&1u32.to_le_bytes());
        block[entry + 16..entry + 16 + name.len()].copy_from_slice(name);
        block[4000] = b'y';

        assert_eq!(
            parse_xattr_block(&block),
            vec![Ext4Xattr {
                name: "user.overlay.whiteout".to_string(),
                value: vec![b'y'],
            }]
        );
    }
}
