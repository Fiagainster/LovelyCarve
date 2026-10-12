# LovelyCarve integration

Vendored reader crate `bitlocker-core` 0.3.5 from
<https://github.com/SecurityRonin/bitlocker-forensic>, commit
`e6674dcda2413b1a02a915f2255af8fda787a154`. Apache-2.0; see LICENSE.
Workspace manifest fields are expanded for a standalone crate. The separate
forensic analyzer and hosted CI are not included.

Local changes:

- Try every protector of the requested kind, requiring authenticated VMK/FVEK
  unwrap, rather than failing on the first nonmatching password protector.
- Reject out-of-range reads, arithmetic overflow and truncated ciphertext;
  never decrypt fabricated ciphertext to apparent plaintext.
- Correct obsolete cipher-support error messages.
- The application adapter provides a bounded, weakly owned source over the
  decoded image; no FFI or whole-volume temporary extraction is involved.
  It rejects non-512-byte sectors because the upstream cipher uses fixed
  512-byte data units. AES-256-CBC + Diffuser remains explicitly unsupported,
  as in upstream; no unvalidated cipher has been enabled.

Tests: `just test-bitlocker`, `just test bde_image --lib`, `just test-cli`.
`BDE_ORACLE_IMAGE` enables the upstream independent oracle over the complete
dfVFS image. Other external oracle tests are optional and skip when absent;
their passing harness status does not imply those datasets were exercised.
The committed minimized projection is described in `tests/data/README.md`.
