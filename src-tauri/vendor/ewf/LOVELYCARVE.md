# Vendored ewf reader

Source: https://github.com/SecurityRonin/ewf-forensic/tree/c456f993f68385f3438b3ff16ba5ab6fd0850fb4/core
Version: ewf 0.4.10. License: Apache-2.0 (see LICENSE).

Only the core reader and its tests are vendored; the forensic integrity analyser,
upstream CLI, workflows and development scripts are not runtime dependencies.
The standalone manifest expands the upstream workspace dependencies.

Local changes:
- Export segment discovery and explicit-path lazy opening for source snapshots.
- Escape parent paths, support uppercase Ex01 extensions and signature-only names.
- Preserve sector geometry, media type, compression, model/serial, and UTF-16 header2.
- Decode EWF2 bzip2 and pattern-fill chunks; retain its per-chunk checksum flags
  without increasing the 16-byte chunk-table entry size.
- Bound decompression, verify uncompressed Adler-32 and compressed stream trailers,
  and reject short/oversized decoded chunks instead of returning fabricated zeroes.
- Bound EWF2 geometry and reject offset overflow rather than shifting chunk indices.
- Fix the upstream no-default-features VerifyResult impl gate.
- Update the synthetic raw-chunk test to include the format's Adler-32 trailer.

Run `just test-ewf` from the repository root. Adapter tests are part of `just test`.
Large external corpus tests remain opt-in via EWF_TEST_CORPUS; they are not claimed
as local validation when those files are absent. Small committed fixtures retain
upstream provenance in tests/data/README.md and the upstream test descriptions.
