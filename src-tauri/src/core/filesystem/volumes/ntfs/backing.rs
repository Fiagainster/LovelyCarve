//! Bounded read and seek adapters for image and BitLocker partitions.
use super::*;

/// Backing source for an NTFS volume. Either a raw partition slice of a
/// `DiskImage` (the common case) or the plaintext stream produced by an
/// unlocked BitLocker volume.
#[derive(Clone)]
pub(super) enum PartitionBacking<'a> {
    Image {
        image: &'a DiskImage,
        base_offset: u64,
        length_bytes: u64,
    },
    Bitlocker {
        unlocked: Arc<UnlockedVolume>,
        length_bytes: u64,
    },
}

impl<'a> PartitionBacking<'a> {
    pub(super) fn length(&self) -> u64 {
        match self {
            Self::Image { length_bytes, .. } => *length_bytes,
            Self::Bitlocker { length_bytes, .. } => *length_bytes,
        }
    }

    /// Absolute image byte offset of this volume's start, for translating
    /// volume-relative data-run positions into image-absolute byte runs.
    /// `None` for BitLocker backings (their plaintext stream has no stable
    /// image offset), which simply skips run capture.
    pub(super) fn base_offset(&self) -> Option<u64> {
        match self {
            Self::Image { base_offset, .. } => Some(*base_offset),
            Self::Bitlocker { .. } => None,
        }
    }

    pub(super) fn read_into(&self, position: u64, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Image {
                image, base_offset, ..
            } => {
                let bytes = image
                    .read_range(base_offset + position, buf.len())
                    .map_err(|err| io::Error::other(err.to_string()))?;
                let copied = bytes.len();
                buf[..copied].copy_from_slice(&bytes);
                Ok(copied)
            }
            Self::Bitlocker { unlocked, .. } => unlocked
                .read_at(position, buf)
                .map_err(|err| io::Error::other(err.to_string())),
        }
    }

    pub(super) fn reader(&self) -> PartitionReader<'a> {
        PartitionReader {
            backing: self.clone(),
            position: 0,
        }
    }
}

pub(super) struct PartitionReader<'a> {
    pub(super) backing: PartitionBacking<'a>,
    pub(super) position: u64,
}

impl Read for PartitionReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let length = self.backing.length();
        if self.position >= length || buf.is_empty() {
            return Ok(0);
        }
        let remaining = length - self.position;
        let want = (buf.len() as u64).min(remaining) as usize;
        let copied = self.backing.read_into(self.position, &mut buf[..want])?;
        self.position += copied as u64;
        Ok(copied)
    }
}

impl Seek for PartitionReader<'_> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let length = self.backing.length();
        let target = match pos {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::Current(delta) => self.position as i128 + delta as i128,
            SeekFrom::End(delta) => length as i128 + delta as i128,
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NTFS partition seek before start",
            ));
        }
        self.position = target as u64;
        Ok(self.position)
    }
}
