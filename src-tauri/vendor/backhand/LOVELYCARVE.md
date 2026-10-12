# LovelyCarve integration

backhand 0.25.5 from crates.io, source bf678c7f923799a57d972cf695daf69d8c6ddafb (MIT OR Apache-2.0).

The standalone manifest trims upstream benches, integration-test harnesses and dev dependencies, retains xz/gzip/zstd/parallel and readable error strings, and enables no_std_io2/std for host I/O compatibility. Other compression features remain optional and disabled in the host. SquashFS root names use the on-disk slash rather than the host platform root separator.

Upstream audit and verification commands: [../UPSTREAM.md](../UPSTREAM.md).
