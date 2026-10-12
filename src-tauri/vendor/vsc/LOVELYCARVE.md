# LovelyCarve integration

Reader crate `vsc-core` 0.2.2 from
<https://github.com/SecurityRonin/vsc-forensic>, commit
`0af70bed6910fef542224ecd94928b4bc0ae920b`. Apache-2.0; see LICENSE.
Workspace manifest fields are expanded for a standalone crate. The separate
forensic analyzer, fuzz targets, hosted CI, the external PC-MUS-001 oracle
tests and the optional `forensic-vfs` bridge are not included.

The application uses `core/vss_image.rs` to list each NTFS volume's shadow
copies and to expose one as a session-only `vss://` image; no temporary
extraction or native DLL is involved.

Local changes:

- Snapshot reconstruction is rewritten to apply the store chain. Upstream
  overlaid each store on the live volume alone, so an older snapshot returned
  post-snapshot bytes for every block first overwritten after a newer snapshot
  was taken (on dfVFS `vss.raw`, 67 of the older store's 5,052 blocks differed
  from pyvshadow, while the newest store matched). The rules now follow the
  libvshadow format documentation: block lists fold forwarders through a
  reverse list, merge overlays and drop not-used and self-forwarding
  descriptors; a read falls through to newer stores, forwarders read the next
  store, overlays apply to the requested store only, and only the newest store
  read directly zero-fills unallocated blocks (current and previous bitmap).
  The module docs in `src/reconstruct.rs` state the full procedure.
- Stores are ordered by creation time, the order the chain is applied in.
- Catalog type-0x03 parsing adds the block-list (+8) and previous-bitmap (+72)
  offsets; the block list is read from that pointer instead of being assumed
  to follow the store header.
- Metadata is validated: a chain block that is out of range, is not a
  version-1 VSS block of the expected record type, is reached twice, or a
  forwarder carrying a store offset is `VssError::CorruptStoreMetadata`.
  Out-of-range data reads and reads past the volume are I/O errors rather than
  zero-filled.
- New owned, `Send + Sync` `StoreMap` / `SnapshotLayout` API so one layout
  serves concurrent positioned readers; `Snapshot` keeps the borrowed API.
- `rust-version` is raised to 1.88 (`Option::is_none_or`, `slice::as_chunks`).

Tests: `just test-vsc` runs the synthetic chain tests;
`VSC_ORACLE_IMAGE=<dfVFS vss.raw> just test-vsc` adds the pyvshadow oracle;
`just test vss_image` covers the application adapter (set `LOVELYCARVE_VSS_TEST`
to `vss.raw` for the end-to-end NTFS check). The random multi-store
differential against pyvshadow is described in `tests/oracle/README.md`.
Optional oracles skip when their data is absent, so a passing run alone does
not mean they were exercised.
