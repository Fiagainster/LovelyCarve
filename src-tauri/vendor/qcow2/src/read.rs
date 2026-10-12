use std::cmp::min;
use std::io;
use std::mem::size_of;

use byteorder::BigEndian;
use positioned_io::{ByteIo, ReadAt, Size};

use super::{Error, Qcow2, Result};

const L1_COW: u64 = 1 << 63;
const L1_RESERVED: u64 = (0x7F << 56) | 0x1FF;
const L1_POS: u64 = !(L1_COW | L1_RESERVED);
#[derive(Debug)]
pub enum L1Entry {
    Empty,
    Standard { pos: u64 },
}

const L2_COW: u64 = 1 << 63;
const L2_COMPRESSED: u64 = 1 << 62;
const L2_ZERO: u64 = 1;
const L2_RESERVED: u64 = (0x3F << 56) | 0xFE;
const L2_POS: u64 = !(L2_COW | L2_COMPRESSED | L2_ZERO | L2_RESERVED);
const L2_COMPRESSED_MASK: u64 = !(L2_COW | L2_COMPRESSED);
#[derive(Debug)]
pub enum L2Entry {
    Empty,
    Standard { pos: u64, zero: bool },
    Compressed,
}

impl<I> Qcow2<I>
where
    I: ReadAt,
{
    /// Get a Reader for the main virtual disk.
    ///
    /// This allows data to be read from inside the virtual disk image.
    pub fn reader(&self) -> Result<Reader<'_, I>> {
        Ok(Reader { q: self })
    }

    fn l1_entry_read(&self, l1: &ByteIo<Vec<u8>, BigEndian>, l1_l2_idx: u64) -> Result<L1Entry> {
        let offset = l1_l2_idx * size_of::<u64>() as u64;
        let entry = l1.read_u64_at(offset)?;
        if entry & L1_RESERVED != 0 {
            return Err(Error::FileFormat(
                "reserved bit used in L1 entry".to_owned(),
            ));
        }

        let pos = entry & L1_POS;
        if pos == 0 {
            return Ok(L1Entry::Empty);
        }
        Ok(L1Entry::Standard { pos })
    }
    fn l2_entry_read_raw(&self, l2_pos: u64, l2_block_idx: u64) -> Result<u64> {
        let offset = l2_pos + l2_block_idx * size_of::<u64>() as u64;

        // Check the cache.
        let mut cache = self.l2_cache.lock()?;
        if let Some(ret) = cache.get(&offset) {
            return Ok(*ret);
        }

        let ret = self.io.read_u64_at(offset)?;
        cache.put(offset, ret);
        Ok(ret)
    }
    fn l2_entry_parse(&self, entry: u64) -> Result<L2Entry> {
        Ok(if entry & L2_COMPRESSED != 0 {
            let x = 70 - self.header.c.cluster_bits;
            let entry = entry & L2_COMPRESSED_MASK;
            let _pos = entry & ((1 << x) - 1);
            let _size = (entry >> x) * 512;
            L2Entry::Compressed
        } else {
            if entry & L2_RESERVED != 0 {
                return Err(Error::FileFormat(
                    "reserved bit used in L2 entry".to_owned(),
                ));
            }
            let pos = entry & L2_POS;
            if pos != 0 {
                L2Entry::Standard {
                    pos,
                    zero: (entry & L2_ZERO != 0),
                }
            } else {
                L2Entry::Empty
            }
        })
    }
    fn l2_entry_read(&self, l1: &ByteIo<Vec<u8>, BigEndian>, guest_offset: u64) -> Result<L2Entry> {
        let (l1_l2_idx, l2_block_idx, _) = self.header.guest_offset_info(guest_offset);
        let l1_entry = self.l1_entry_read(l1, l1_l2_idx)?;
        Ok(match l1_entry {
            L1Entry::Empty => L2Entry::Empty,
            L1Entry::Standard { pos, .. } => {
                let raw = self.l2_entry_read_raw(pos, l2_block_idx)?;
                self.l2_entry_parse(raw)?
            }
        })
    }
    fn zero_fill(buf: &mut [u8]) {
        buf.fill(0);
    }
    fn guest_block_read(&self, entry: L2Entry, offset: u64, buf: &mut [u8]) -> Result<()> {
        match entry {
            L2Entry::Empty => Self::zero_fill(buf),
            L2Entry::Standard { pos, zero, .. } => {
                if zero {
                    Self::zero_fill(buf)
                } else {
                    self.io.read_exact_at(pos + offset, buf)?
                }
            }
            L2Entry::Compressed => {
                return Err(Error::UnsupportedFeature("compressed blocks".to_owned()))
            }
        }
        Ok(())
    }
    pub(crate) fn guest_read(
        &self,
        l1: &ByteIo<Vec<u8>, BigEndian>,
        pos: u64,
        buf: &mut [u8],
    ) -> io::Result<usize> {
        // Check for reads past EOF.
        if pos >= self.header.guest_size() {
            return Ok(0);
        }
        let ret = min(buf.len() as u64, self.header.guest_size() - pos) as usize;
        let mut buf = &mut buf[..ret];

        let mut offset = pos % self.cluster_size();
        let mut guest_block_pos = pos - offset;
        while !buf.is_empty() {
            let entry = self.l2_entry_read(l1, guest_block_pos)?;
            let size = min(buf.len() as u64, self.cluster_size() - offset) as usize;
            self.guest_block_read(entry, offset, &mut buf[..size])?;

            let tmp = buf;
            buf = &mut tmp[size..];
            guest_block_pos += self.cluster_size();
            offset = 0;
        }
        Ok(ret)
    }

    pub(crate) fn l1_read(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0; self.header.l1_entries() as usize * size_of::<u64>()];
        self.io
            .read_exact_at(self.header.c.l1_table_offset, &mut buf)?;
        Ok(buf)
    }
}

/// A reader of data from the virtual disk image.
pub struct Reader<'a, I: 'a + ReadAt> {
    q: &'a Qcow2<I>,
}

impl<'a, I> ReadAt for Reader<'a, I>
where
    I: 'a + ReadAt,
{
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.q.read_at(pos, buf).map_err(Into::into)
    }
}

impl<'a, I> Size for Reader<'a, I>
where
    I: 'a + ReadAt,
{
    fn size(&self) -> io::Result<Option<u64>> {
        Ok(Some(self.q.guest_size()))
    }
}
