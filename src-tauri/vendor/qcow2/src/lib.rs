#![warn(missing_docs)]

//! This crate can parse and read qcow2 virtual disks, as used by qemu and other emulators.
//!
//! [Qcow2](https://en.wikipedia.org/wiki/Qcow) is a flexible format for disk images, that only
//! allocates space as needed. It has many other interesting features.
//!
//! The following features are supported by this crate:
//!
//!  * Reading blocks of virtual disk data.
//!  * Reading data that is not aligned to block boundaries.
//!  * Parsing and validation of the header.
//!  * Reporting the names of any unsupported features, using the "feature name table" extension.
//!  * Basic caching of guest data locations, so nearby reads will be fast.
//!
//! These features are not yet supported, but should be easy to add:
//!
//! * Listing and reading snapshots.
//! * Reading compressed data.
//! * Backing file support, so you can chain qcow2 files together.
//! * Reporting information about images.
//!
//! These features are harder, or less interesting to me. Patches welcome!
//!
//! * Reading encrypted qcow2 files.
//! * Writing virtual disk data.
//! * Repairing the disk if refcounts are out of date.
//! * Compacting the virtual disk so it takes less space.
//! * Maintaining a "dirty bitmap" to make backups faster.
//! * Creating new qcow2 images.
//! * Creating new snapshots.
//! * Checking qcow2 images for inconsistencies.
//! * Merging images into their backing file.
//! * Resizing images.
//!
//! The repository for this crate is at https://github.com/vasi/qcow2-rs

extern crate byteorder;
extern crate lru;
extern crate positioned_io;

mod error;
mod extension;
mod feature;
mod header;
mod int;
mod read;
pub use crate::error::Error;
pub use crate::read::Reader;

use std::fmt::{self, Debug, Formatter};
use std::num::NonZeroUsize;
use std::result;
use std::sync::Mutex;

use byteorder::BigEndian;
use lru::LruCache;
use positioned_io::{ByteIo, ReadAt};

const L2_CACHE_SIZE: usize = 32;

/// A qcow2 image.
///
/// # Examples
///
/// ```no_run
/// extern crate positioned_io;
/// extern crate qcow2;
///
/// # use std::fs::File;
/// use positioned_io::ReadAt;
/// use qcow2::Qcow2;
///
/// # fn foo() -> qcow2::Result<()> {
///
/// // Open a file.
/// let file = File::open("image.qcow2")?;
/// let qcow = Qcow2::open(file)?;
///
/// // Read some data.
/// let reader = qcow.reader()?;
/// let mut buf = vec![0; 4096];
/// reader.read_exact_at(5 * 1024 * 1024, &mut buf)?;
///
/// # Ok(()) } fn main() { foo().unwrap(); }
/// ```
pub struct Qcow2<I>
where
    I: ReadAt,
{
    header: header::Header,
    io: ByteIo<I, BigEndian>,
    l1: ByteIo<Vec<u8>, BigEndian>,

    l2_cache: Mutex<LruCache<u64, u64>>,
}

/// The result type for operations on qcow2 images.
pub type Result<T> = std::result::Result<T, Error>;

impl<I> Qcow2<I>
where
    I: ReadAt,
{
    /// Open a source of data as a qcow2 image.
    ///
    /// Usually the data source `io` will be a file.
    pub fn open(io: I) -> Result<Self> {
        let io: ByteIo<_, BigEndian> = ByteIo::new(io);
        let l2_cache_size = NonZeroUsize::new(L2_CACHE_SIZE)
            .ok_or_else(|| Error::Internal("L2 cache size must be non-zero".to_owned()))?;
        let mut q = Qcow2 {
            header: Default::default(),
            io,
            l1: ByteIo::new(Vec::new()),
            l2_cache: Mutex::new(LruCache::new(l2_cache_size)),
        };
        q.header.read(&mut q.io)?;
        q.l1 = ByteIo::new(q.l1_read()?);
        Ok(q)
    }

    /// Get the size of each block of this qcow2 image.
    pub fn cluster_size(&self) -> u64 {
        self.header.cluster_size()
    }

    /// Get the size of the virtual image.
    ///
    /// This is likely to differ from the size of the qcow2 file itself, since the file can grow.
    pub fn guest_size(&self) -> u64 {
        self.header.guest_size()
    }

    /// Read bytes at an offset in the virtual disk.
    ///
    /// The L1 table is loaded once when the image is opened, so repeated
    /// filesystem reads do not re-read the complete table from disk.
    pub fn read_at(&self, pos: u64, buf: &mut [u8]) -> Result<usize> {
        Ok(self.guest_read(&self.l1, pos, buf)?)
    }
}

impl<I> Debug for Qcow2<I>
where
    I: ReadAt,
{
    fn fmt(&self, f: &mut Formatter) -> result::Result<(), fmt::Error> {
        f.debug_struct("Qcow2")
            .field("header", &self.header)
            .finish()
    }
}
