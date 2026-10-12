//! 文本「原类型视图 / 文本视图」的字符编码检测 + 解码。前端此前一律按 UTF-8
//! 解码原始字节，GBK/GB18030/Big5/Shift-JIS/EUC-* 等非 UTF-8 文本会被解成乱码。
//! 这里改为先用 chardetng（Firefox 的遗留内容编码检测器）猜出编码，再用
//! encoding_rs 解码为 UTF-8 String 回传，前端直接渲染。
//!
//! 检测策略：BOM 权威优先（UTF-8 / UTF-16LE / UTF-16BE 由文件头声明，胜过统计
//! 猜测），无 BOM 时交给 chardetng 统计检测。chardetng 1.0 不给数值置信度，故以
//! `from_bom`（是否 BOM 声明）+ `had_errors`（解码是否遇到无法映射的字节）作为
//! 「这次猜测有多可信」的信号——前端据此弱化提示并允许用户手动改编码。

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;
use serde::Serialize;

/// 一次文本预览的解码结果。`text` 已是 UTF-8，前端文本/代码/Markdown/结构化
/// 视图直接渲染；`encoding` 等字段驱动「编码」徽标与覆盖入口。
#[derive(Debug, Serialize)]
pub struct DecodedText {
    /// 用检测出的（或 BOM 声明的）编码解码而成的 UTF-8 文本。
    pub text: String,
    /// 实际使用的编码 WHATWG 规范名，如 `UTF-8` / `GBK` / `Big5` / `Shift_JIS`。
    pub encoding: String,
    /// 编码来自文件头 BOM（权威）而非统计检测——UI 据此把「自动检测」表述为「BOM 声明」。
    pub from_bom: bool,
    /// 解码过程中出现过该编码无法映射的字节（已用 U+FFFD 替换）。检测误判或本是
    /// 二进制内容时为 true，是「若仍乱码请手动改编码」的提示信号。
    pub had_errors: bool,
}

/// 检测 `bytes` 的字符编码并解码为 UTF-8。空输入按空 UTF-8 处理。
pub fn decode(bytes: &[u8]) -> DecodedText {
    if bytes.is_empty() {
        return DecodedText {
            text: String::new(),
            encoding: encoding_rs::UTF_8.name().to_owned(),
            from_bom: false,
            had_errors: false,
        };
    }

    // 1) BOM 权威：UTF-8 / UTF-16LE / UTF-16BE 由文件头声明，直接采用。
    //    encoding_rs 的 decode() 会自行识别并剥掉 BOM，故输出不含前导 BOM 字符。
    if let Some((enc, _bom_len)) = Encoding::for_bom(bytes) {
        let (text, _used, had_errors) = enc.decode(bytes);
        return DecodedText {
            text: text.into_owned(),
            encoding: enc.name().to_owned(),
            from_bom: true,
            had_errors,
        };
    }

    // 2) 无 BOM：chardetng 统计检测。允许 UTF-8 与 ISO-2022-JP 作为结果——这是取证
    //    查看器而非会执行脚本的浏览器，不存在「检测 UTF-8 会被网页滥用」的顾虑，
    //    检测面越宽越好。单块喂入（last = true）。
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    detector.feed(bytes, true);
    let enc = detector.guess(None, Utf8Detection::Allow);

    // 3) 用猜出的编码解码；非法序列被替换为 U+FFFD，并经 had_errors 反馈。
    let (text, used, had_errors) = enc.decode(bytes);
    DecodedText {
        text: text.into_owned(),
        encoding: used.name().to_owned(),
        from_bom: false,
        had_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_ascii_is_utf8_clean() {
        let r = decode(b"hello, world\n");
        assert_eq!(r.text, "hello, world\n");
        assert_eq!(r.encoding, "UTF-8");
        assert!(!r.from_bom);
        assert!(!r.had_errors);
    }

    #[test]
    fn utf8_bom_is_detected_and_stripped() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("中文".as_bytes());
        let r = decode(&bytes);
        assert_eq!(r.text, "中文");
        assert_eq!(r.encoding, "UTF-8");
        assert!(r.from_bom);
        assert!(!r.had_errors);
    }

    #[test]
    fn gbk_chinese_decodes_without_mojibake() {
        // “你好世界” in GBK (each char 2 bytes), no BOM.
        let gbk = [0xC4, 0xE3, 0xBA, 0xC3, 0xCA, 0xC0, 0xBD, 0xE7];
        let r = decode(&gbk);
        assert_eq!(r.text, "你好世界");
        assert!(!r.from_bom);
        // GBK/GB18030 family.
        assert!(
            r.encoding.eq_ignore_ascii_case("gbk") || r.encoding.eq_ignore_ascii_case("gb18030"),
            "unexpected encoding: {}",
            r.encoding
        );
    }

    #[test]
    fn utf16le_bom_decodes() {
        // "AB" in UTF-16LE with BOM.
        let bytes = [0xFF, 0xFE, b'A', 0x00, b'B', 0x00];
        let r = decode(&bytes);
        assert_eq!(r.text, "AB");
        assert!(r.from_bom);
        assert_eq!(r.encoding, "UTF-16LE");
    }
}
