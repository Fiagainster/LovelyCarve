//! Strict configuration text decoding with original-byte offsets.
use lovelycarve_plugin::{Error, Result};
pub struct Text {
    pub contents: String,
    pub encoding: &'static str,
    bom: usize,
    utf16: bool,
}
impl Text {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
            return Err(Error::new("WEB_ENCODING", "UTF-32 不支持"));
        }
        let (bom, encoding, utf16, big_endian) = if bytes.starts_with(&[0xff, 0xfe]) {
            (2, "UTF-16LE BOM", true, false)
        } else if bytes.starts_with(&[0xfe, 0xff]) {
            (2, "UTF-16BE BOM", true, true)
        } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            (3, "UTF-8 BOM", false, false)
        } else {
            (0, "UTF-8", false, false)
        };
        let body = bytes
            .get(bom..)
            .ok_or_else(|| Error::new("WEB_ENCODING", "BOM 短读"))?;
        let contents = if utf16 {
            if !body.len().is_multiple_of(2) {
                return Err(Error::new("WEB_ENCODING", "UTF-16 奇数字节"));
            }
            let units: Vec<_> = body
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    if big_endian {
                        u16::from_be_bytes(*pair)
                    } else {
                        u16::from_le_bytes(*pair)
                    }
                })
                .collect();
            String::from_utf16(&units)
                .map_err(|_| Error::new("WEB_ENCODING", "UTF-16 代理项损坏"))?
        } else {
            std::str::from_utf8(body)
                .map_err(|_| Error::new("WEB_ENCODING", "UTF-8 损坏，未猜测代码页"))?
                .to_owned()
        };
        if contents.len() > 256 * 1024 {
            return Err(Error::new("RESOURCE_LIMIT", "配置文本超过 256 KiB"));
        }
        Ok(Self {
            contents,
            encoding,
            bom,
            utf16,
        })
    }
    pub fn offset(&self, index: usize) -> Result<usize> {
        let prefix = self
            .contents
            .get(..index)
            .ok_or_else(|| Error::new("WEB_ENCODING", "文本偏移不是字符边界"))?;
        Ok(self.bom
            + if self.utf16 {
                prefix.encode_utf16().count() * 2
            } else {
                index
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_bom_and_surrogate_pairs_keep_original_spans() -> Result<()> {
        let raw = "Name 中文😀\r\nRoot x\r\n";
        for (bom, utf16, big) in [
            (vec![0xef, 0xbb, 0xbf], false, false),
            (vec![0xff, 0xfe], true, false),
            (vec![0xfe, 0xff], true, true),
        ] {
            let mut bytes = bom.clone();
            if utf16 {
                bytes.extend(raw.encode_utf16().flat_map(|unit| {
                    if big {
                        unit.to_be_bytes()
                    } else {
                        unit.to_le_bytes()
                    }
                }));
            } else {
                bytes.extend(raw.as_bytes());
            }
            let text = Text::decode(&bytes)?;
            assert_eq!(text.contents, raw);
            let boundary = raw
                .find("Root")
                .ok_or_else(|| Error::new("TEST", "boundary"))?;
            let offset = text.offset(boundary)?;
            let expected = if utf16 {
                b"R\0".to_vec()
            } else {
                b"R".to_vec()
            };
            assert_eq!(
                &bytes[offset..offset + expected.len()],
                if big { b"\0R" } else { expected.as_slice() }
            );
            assert_eq!(text.offset(raw.len())?, bytes.len());
        }
        for bytes in [
            &[0xff][..],
            &[0xff, 0xfe, 1][..],
            &[0xff, 0xfe, 0, 0xd8][..],
            &[0xff, 0xfe, 0, 0][..],
        ] {
            assert!(Text::decode(bytes).is_err());
        }
        Ok(())
    }
}
