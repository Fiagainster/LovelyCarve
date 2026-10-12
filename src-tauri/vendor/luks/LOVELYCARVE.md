# LovelyCarve integration

Vendored core from https://github.com/SecurityRonin/luks-forensic at `284f06487d7a66f13ed27c61be5e26f0863c0e4f`. Apache-2.0; original license and tests retained.

Local changes: standalone manifest; reject out-of-range/truncated encrypted data instead of synthesizing ciphertext. Application integration adds bounded evidence access and metadata validation.

Header allocation is limited to 8 KiB–4 MiB and the available source length. The application additionally verifies the LUKS2 primary checksum and rejects unsupported segment/keyslot layouts before derivation. Fixed-size/multi-segment data and backup-header recovery are not supported.
