# Minimized BitLocker regression evidence

`bdetogo-projection.bin.gz` is a **partial projection**, not a complete disk
image, of the Apache-2.0 dfVFS `bdetogo.raw` fixture by Joachim Metz and the
dfVFS project. Source:
<https://raw.githubusercontent.com/log2timeline/dfvfs/main/test_data/bdetogo.raw>.
The complete source is 67,108,864 bytes, MD5
`fcba22f9363388101ae66c741588bc45`; its published password is `bde-TEST`.

`bdetogo-projection.json` records the exact retained ciphertext ranges and
SHA-256 of the expanded projection. All other bytes are zeroed. Retained
ranges contain the original header, all three metadata blocks and the entire
relocated header region (which includes the oracle's logical test offsets).
Projection generation copies these ranges unchanged into a zero-filled buffer
of the original length, then gzip-compresses it with `mtime=0`.

Only retained sectors are valid oracle inputs. Tests must not compare a full
export hash to the complete image or treat omitted ciphertext as evidence.
The adapter and CLI tests compare known logical sectors against the independent
pybde SHA-256 values published in upstream `core/tests/oracle_bdetogo.rs`.
The complete unmodified image is separately tested with `BDE_ORACLE_IMAGE`.

`recovery-cbc.bin` is a synthetic AES-128-CBC / recovery-password fixture,
independently encrypted with PyCryptodome by `generate_recovery_fixture.py`.
Its test-only recovery password is eight `111111` groups. The expected relocated
boot plaintext is `bytes((i & 255) ^ 0x3c for i in range(512))`. This checks
crypto interoperability and adapter dispatch, not a Windows-issued volume.
