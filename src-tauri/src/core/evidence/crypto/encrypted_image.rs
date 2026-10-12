//! Session-only plaintext views over bounded, read-only evidence sources.

use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, Weak};

use anyhow::{anyhow, bail, ensure, Context};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::core::evidence::image::{DerivedImage, DiskImage, ImageInfo};
use crate::core::evidence::partitions::{detect_partitions_with_meta, SectorSource};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EncryptionScheme {
    Luks,
    Veracrypt,
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UnlockTarget {
    #[default]
    NewImage,
    SourcePartition,
}

// Intentionally no Debug or Serialize: credentials must not enter diagnostics.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnlockOptions {
    #[serde(default)]
    pub target: UnlockTarget,
    pub scheme: EncryptionScheme,
    pub password: String,
    pub partition_id: Option<u8>,
    pub offset: Option<u64>,
    pub length: Option<u64>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub pim: u32,
}

#[derive(Serialize)]
pub struct UnlockResult {
    #[serde(skip)]
    pub(crate) read_cancellation: CancellationToken,
    pub image: ImageInfo,
    pub scheme: EncryptionScheme,
    pub cipher: String,
    pub encryption_sector_size: usize,
    pub source_offset: u64,
    pub source_length: u64,
    pub hidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_key: Option<String>,
}

struct EvidenceReader {
    source: Weak<DiskImage>,
    offset: u64,
    length: u64,
    position: u64,
    cancellation: Arc<Mutex<CancellationToken>>,
}

impl Read for EvidenceReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self
            .cancellation
            .lock()
            .map_err(|_| std::io::Error::other("encrypted cancellation lock poisoned"))?
            .is_cancelled()
        {
            return Err(std::io::Error::other(
                "encrypted volume operation cancelled",
            ));
        }
        let source = self
            .source
            .upgrade()
            .ok_or_else(|| std::io::Error::other("encrypted evidence closed"))?;
        let count = buffer
            .len()
            .min(self.length.saturating_sub(self.position) as usize);
        if count == 0 {
            return Ok(0);
        }
        let offset = self.offset + self.position;
        let bytes = source
            .read_range(offset, count)
            .map_err(std::io::Error::other)?;
        if bytes.len() != count
            || source.bad_regions().iter().any(|region| {
                region.offset < offset + count as u64
                    && region.offset.saturating_add(region.len) > offset
            })
        {
            return Err(std::io::Error::other(
                "encrypted evidence contains unreadable ciphertext",
            ));
        }
        buffer[..count].copy_from_slice(&bytes);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for EvidenceReader {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        if self
            .cancellation
            .lock()
            .map_err(|_| std::io::Error::other("encrypted cancellation lock poisoned"))?
            .is_cancelled()
        {
            return Err(std::io::Error::other(
                "encrypted volume operation cancelled",
            ));
        }
        let position = match from {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
            SeekFrom::End(delta) => i128::from(self.length) + i128::from(delta),
        };
        self.position = u64::try_from(position)
            .map_err(|_| std::io::Error::other("invalid encrypted evidence seek"))?;
        Ok(self.position)
    }
}

enum PlaintextReader {
    Luks(luks::DecryptedPayload<EvidenceReader>),
    Veracrypt(veracrypt::DecryptedVolume<EvidenceReader>),
}

struct PlaintextSource {
    reader: Mutex<PlaintextReader>,
    length: u64,
}

impl SectorSource for PlaintextSource {
    fn size(&self) -> u64 {
        self.length
    }

    fn read(&self, offset: u64, length: usize) -> anyhow::Result<Vec<u8>> {
        let length = (length as u64).min(self.length.saturating_sub(offset)) as usize;
        let mut bytes = vec![0; length];
        let mut reader = self
            .reader
            .lock()
            .map_err(|_| anyhow!("decrypted reader lock poisoned"))?;
        match &mut *reader {
            PlaintextReader::Luks(reader) => reader.read_at(offset.min(self.length), &mut bytes)?,
            PlaintextReader::Veracrypt(reader) => {
                reader.read_at(offset.min(self.length), &mut bytes)?
            }
        }
        Ok(bytes)
    }
}

pub fn unlock(
    source: &Arc<DiskImage>,
    options: UnlockOptions,
    cancellation: CancellationToken,
) -> anyhow::Result<(DiskImage, UnlockResult)> {
    ensure!(
        !cancellation.is_cancelled(),
        "encrypted volume operation cancelled"
    );
    ensure!(!options.password.is_empty(), "password must not be empty");
    ensure!(
        options.password.len() <= 4096,
        "password exceeds 4096 bytes"
    );
    ensure!(
        options.pim <= 1000,
        "PIM above 1000 exceeds the supported derivation budget"
    );
    let (offset, length) = if let Some(id) = options.partition_id {
        ensure!(
            options.offset.is_none() && options.length.is_none(),
            "partitionId cannot be combined with offset/length"
        );
        let partition = source
            .partitions
            .iter()
            .find(|partition| partition.id == id)
            .context("partition not found")?;
        (
            partition
                .start_lba
                .checked_mul(source.sector_size as u64)
                .context("partition offset overflow")?,
            partition
                .length_lba
                .checked_mul(source.sector_size as u64)
                .context("partition size overflow")?,
        )
    } else {
        let offset = options.offset.unwrap_or(0);
        (
            offset,
            options
                .length
                .unwrap_or_else(|| source.len().saturating_sub(offset)),
        )
    };
    ensure!(
        length >= 512
            && offset
                .checked_add(length)
                .is_some_and(|end| end <= source.len()),
        "encrypted region is outside the image"
    );
    let read_cancellation = Arc::new(Mutex::new(cancellation.clone()));
    let mut reader = EvidenceReader {
        source: Arc::downgrade(source),
        offset,
        length,
        position: 0,
        cancellation: Arc::clone(&read_cancellation),
    };
    let source_length = length;
    let (reader, length, cipher, encryption_sector_size) = match options.scheme {
        EncryptionScheme::Luks => {
            ensure!(
                !options.hidden && options.pim == 0,
                "hidden/PIM options apply only to VeraCrypt"
            );
            crate::core::evidence::crypto::luks_metadata::validate(&mut reader, length)?;
            reader.rewind()?;
            let volume =
                luks::LuksVolume::unlock_with_passphrase(reader, options.password.as_bytes())?;
            let size = volume.payload_size();
            let info = volume.info();
            ensure!(
                size > 0 && size % info.sector_size as u64 == 0,
                "invalid LUKS payload geometry"
            );
            let cipher = format!(
                "LUKS{} {}-{}",
                info.version, info.cipher_name, info.cipher_mode
            );
            let sector = info.sector_size;
            (PlaintextReader::Luks(volume), size, cipher, sector)
        }
        EncryptionScheme::Veracrypt => {
            let volume = if options.hidden {
                veracrypt::VeraVolume::unlock_hidden_with_pim(
                    reader,
                    options.password.as_bytes(),
                    options.pim,
                )?
            } else {
                veracrypt::VeraVolume::unlock_with_pim(
                    reader,
                    options.password.as_bytes(),
                    options.pim,
                )?
            };
            let size = volume.data_size();
            let cipher = format!(
                "{} / {}",
                volume.info().cipher_display(),
                volume.info().prf.name()
            );
            (PlaintextReader::Veracrypt(volume), size, cipher, 512)
        }
    };
    if cancellation.is_cancelled() {
        bail!("encrypted volume operation cancelled");
    }
    let plaintext = Arc::new(PlaintextSource {
        reader: Mutex::new(reader),
        length,
    });
    let detected = detect_partitions_with_meta(plaintext.as_ref(), 512)?;
    let mut image =
        DiskImage::from_assembled("decrypted".into(), plaintext, detected.partitions, 512)?;
    image.path = format!("decrypted://{}", image.id).into();
    image.derived = Some(DerivedImage::Decrypted {
        parent: source.id.clone(),
    });
    image.table_scheme = detected.scheme;
    image.disk_id = detected.disk_id;
    ensure!(
        !cancellation.is_cancelled(),
        "encrypted volume operation cancelled"
    );
    // Completed views have their own lifetime; a late unlock cancel must not relock them.
    let lifetime = CancellationToken::new();
    *read_cancellation
        .lock()
        .map_err(|_| anyhow!("encrypted cancellation lock poisoned"))? = lifetime.clone();
    let result = UnlockResult {
        read_cancellation: lifetime,
        image: image.info(),
        scheme: options.scheme,
        cipher,
        encryption_sector_size,
        source_offset: offset,
        source_length,
        hidden: options.hidden,
        volume_key: None,
    };
    Ok((image, result))
}
