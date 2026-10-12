# LovelyCarve integration

xfuse source audited at KhaledEmaraDev/xfuse 2ea4e250db00d3f0f845499c8a83f85b9a77b7f8 (BSD-2-Clause).

Portable read-only fork with no FUSE server or Unix ABI. Includes upstream v1 inode link-count/attribute support and real-time flags/superblock parsing. Preserves explicit invalid-inode errors, portable FileAttr, per-volume source and guarded panic conversion. A real-time inode is rejected because the host API has no independent real-time device source; it must never read those addresses from the data device. The upstream service-level file cache and FUSE methods are not part of this fork.

Upstream audit and verification commands: [../UPSTREAM.md](../UPSTREAM.md).
