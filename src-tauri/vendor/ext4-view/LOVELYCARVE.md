# LovelyCarve integration

ext4-view 1.0.0 from crates.io, source 8facbf4c2986b7e2dbdef92514d696f1559a2b90 (MIT OR Apache-2.0).

Preserved local inode-index APIs (block_size, read_raw_inode, open_inode_by_index, read_dir_inode, DirEntry::inode_index), 128-byte inode metadata-checksum handling and tolerant read-only legacy journal compatibility. Unit-test fixture decompression uses Rust zstd rather than a system command; paths are relative to CARGO_MANIFEST_DIR. See test_data/README.md for fixture hashes.

Upstream audit and verification commands: [../UPSTREAM.md](../UPSTREAM.md).
