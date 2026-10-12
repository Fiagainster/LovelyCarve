//! Cross-platform read-only XFS reader, vendored from `xfuse` and stripped of
//! FUSE / Unix-only deps so it can be linked into a Tauri Windows build.
//!
//! The on-disk parser (B+Tree, bmap, superblock, dinode walks, directory
//! decoders, attribute fork) is preserved verbatim; only the host-IO bridge
//! has been rewritten:
//!
//! - `BlockReader` is generic over any `Read + Seek` source rather than a
//!   `std::fs::File`, so an EWF-backed cursor can drop straight in.
//! - `Volume` exposes a small, blocking, panic-tolerant API: `open`, `lookup`,
//!   `getattr`, `readdir`, `read`, `readlink`. The FUSE `Filesystem` trait
//!   impl was deleted along with the `fuser` dependency.
//! - File names are passed and returned as `&[u8]` / `Vec<u8>` (XFS's
//!   on-disk encoding) instead of `OsStr` / `OsString`. The Unix-only
//!   `OsStrExt` / `OsStringExt` shims are gone; callers do their own
//!   bytes ↔ display-string conversion at the UI boundary.
//!
//! Dead-code lints are silenced crate-wide because the on-disk decoders
//! are kept verbatim from upstream `xfuse`: many fields, methods, and
//! variants exist only because they're referenced by the FUSE-side trait
//! impls we deleted, or because they round-trip metadata we don't yet
//! surface. Touching them risks subtle decoding regressions, and our
//! integration-level tests (real-image roundtrip in `volumes::xfs`) would
//! catch any future breakage if we re-enabled the unused parts.
#![allow(dead_code)]

mod libxfuse;

pub use libxfuse::block_reader::BlockReader;
pub use libxfuse::compat::{
    posix_mode_to_file_type, FileAttr, FileType, S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK,
    S_IFMT, S_IFREG, S_IFSOCK,
};
pub use libxfuse::definitions::XfsIno;
pub use libxfuse::dir3::{
    XFS_DIR3_FT_BLKDEV, XFS_DIR3_FT_CHRDEV, XFS_DIR3_FT_DIR, XFS_DIR3_FT_FIFO,
    XFS_DIR3_FT_REG_FILE, XFS_DIR3_FT_SOCK, XFS_DIR3_FT_SYMLINK,
};
pub use libxfuse::sb::Sb;
pub use libxfuse::volume::{DirEntry, Volume};
