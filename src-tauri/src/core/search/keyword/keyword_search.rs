//! 逻辑搜索(文件方式)的多编码关键词与正则匹配引擎。
//!
//! 对每个关键词按编码页（ASCII / UTF-8 / UTF-16LE / UTF-16BE / GBK，可由
//! 调用方选定子集）各自建一台 Aho-Corasick 自动机，对文件字节流式扫描；
//! 正则模式经 [`crate::core::search::keyword::keyword_regex`] 通道在同一遍扫描里合并。行为要点：
//!
//! - **逐编码自动机**：关键词先按编码转成字节模式再做字节级匹配。不区分大小写
//!   时不直接折叠缓冲区找 `ascii_case_insensitive`（GBK 尾字节 0x41–0x7A、
//!   UTF-16 高字节都与字母区间重叠，盲目折叠会误伤中文 / 双字节数据），而是对
//!   **纯 ASCII 关键词**另建折叠自动机扫描 ASCII 折叠后的缓冲区，任意大小写
//!   （「Password」「PASSWORD」）都命中「password」——见 [`MatcherSet::build`]。
//! - **流式分块**：调用方把上一块末尾的 carry 窗口（见 [`MatcherSet::carry_len`]）
//!   拼在新块前一起扫描。完全落在 carry 区内的命中是上一块已报告过的重复，
//!   丢弃；跨越 carry / 新块边界的命中（start+len > carry_len）只在本块出现，
//!   保留——保证每个命中恰好报告一次。
//! - **跨编码按区间去重**：同一 `(start, len)` 可能被多种编码的自动机同时命中
//!   （纯 ASCII 关键词在 ASCII 与 UTF-8 下字节完全相同）；按
//!   [`SearchEncoding::ALL`] 优先级只保留最高优先级编码的命中。关键词与正则
//!   之间互不去重（同一区间被两种通道命中即两条不同模式的记录）。

use std::collections::HashSet;

/// 关键词的一种字节编码。优先级顺序见 [`SearchEncoding::ALL`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchEncoding {
    Ascii,
    Utf8,
    Utf16Le,
    Utf16Be,
    Gbk,
}

impl SearchEncoding {
    /// 全部编码，按优先级排列：同一字节区间被多种编码命中时，靠前的编码获胜。
    pub const ALL: [SearchEncoding; 5] = [
        SearchEncoding::Ascii,
        SearchEncoding::Utf8,
        SearchEncoding::Utf16Le,
        SearchEncoding::Utf16Be,
        SearchEncoding::Gbk,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ascii => "ASCII",
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Gbk => "GBK",
        }
    }

    /// 解析编码页标签：ASCII / UTF-8 / GBK / UTF-16LE / UTF-16BE，大小写不敏感。
    pub fn from_label(label: &str) -> Option<Self> {
        match label.trim().to_ascii_lowercase().as_str() {
            "ascii" => Some(Self::Ascii),
            "utf-8" | "utf8" => Some(Self::Utf8),
            "utf-16le" | "utf16le" => Some(Self::Utf16Le),
            "utf-16be" | "utf16be" => Some(Self::Utf16Be),
            "gbk" => Some(Self::Gbk),
            _ => None,
        }
    }

    /// 把文本编码为该编码下的字节。ASCII 仅对全 ASCII 输入字节精确 —— 构建时
    /// 已用严格检查过滤，非 ASCII 关键词不会进入 ASCII 自动机（此处走 UTF-8
    /// 编码路径，对全 ASCII 输入与 ASCII 字节完全相同）。
    pub fn encode(self, text: &str) -> Vec<u8> {
        match self {
            Self::Ascii | Self::Utf8 => encoding_rs::UTF_8.encode(text).0.into_owned(),
            Self::Utf16Le => utf16_bytes(text, true),
            Self::Utf16Be => utf16_bytes(text, false),
            Self::Gbk => encoding_rs::GBK.encode(text).0.into_owned(),
        }
    }

    /// 把字节解码回字符串（命中上下文预览、正则文本通道用）。残缺序列以替换
    /// 字符容忍，不 panic：半个多字节字符的尾巴、奇数长度的 UTF-16 字节、残缺
    /// 代理对都安全。不做 BOM 嗅探：以 `FF FE` / `EF BB BF` 开头的块仍按本编码
    /// 解码，字符与原字节一一对应（正则通道按字符回推字节偏移依赖这一点）。
    pub fn decode_to_string(self, bytes: &[u8]) -> String {
        match self {
            Self::Ascii | Self::Utf8 => encoding_rs::UTF_8
                .decode_without_bom_handling(bytes)
                .0
                .into_owned(),
            Self::Utf16Le => decode_utf16_bytes(bytes, true),
            Self::Utf16Be => decode_utf16_bytes(bytes, false),
            Self::Gbk => encoding_rs::GBK
                .decode_without_bom_handling(bytes)
                .0
                .into_owned(),
        }
    }
}

/// UTF-16 文本 → 字节（`little_endian` 决定字节序，按码元逐个展开）。
fn utf16_bytes(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        let bytes = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        out.extend_from_slice(&bytes);
    }
    out
}

/// UTF-16 字节 → 文本；奇数尾字节丢弃，残缺代理对以 U+FFFD 填充。
fn decode_utf16_bytes(bytes: &[u8], little_endian: bool) -> String {
    let units = bytes
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect::<Vec<_>>();
    char::decode_utf16(units)
        .map(|result| result.unwrap_or('\u{FFFD}'))
        .collect()
}

/// 一次命中。`start` 是缓冲区相对偏移（buffer = carry ∪ 新块），文件绝对偏移
/// 由调用方自行换算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteHit {
    pub start: usize,
    pub len: usize,
    pub keyword_index: usize,
    pub encoding: SearchEncoding,
}

/// 复用去重集合的容量上限（见 [`MatcherSet::scan_chunk_with`]）。
const MATCH_SCRATCH_RANGES_KEEP: usize = 1024;

/// [`MatcherSet::scan_chunk_with`] 的逐块复用缓冲：大小写不敏感扫描的 ASCII
/// 折叠副本与跨编码区间去重集合。
#[derive(Debug, Default)]
pub struct MatchScratch {
    folded: Vec<u8>,
    seen_ranges: HashSet<(usize, usize)>,
}

/// 单编码的 Aho-Corasick 自动机：`pattern_keyword[i]` 给出第 i 个模式对应的
/// 去重后关键词下标。折叠自动机（扫描 ASCII 折叠缓冲区）单独放在
/// `MatcherSet::folded_matchers` 里，本结构无需标记。
struct EncodingMatcher {
    encoding: SearchEncoding,
    ac: aho_corasick::AhoCorasick,
    pattern_keyword: Vec<usize>,
}

/// 一组关键词（+可选正则）的多编码匹配器（内部不透明）。命中流的模式
/// 下标统一编号：关键词在前（去重后）、正则在后。
pub struct MatcherSet {
    /// 逐编码的常规自动机（原样缓冲区）。
    matchers: Vec<EncodingMatcher>,
    /// 大小写不敏感时逐编码的折叠自动机（ASCII 折叠缓冲区）——只含纯 ASCII
    /// 关键词，见 [`MatcherSet::build`]。
    folded_matchers: Vec<EncodingMatcher>,
    /// 正则通道，模式下标接在关键词之后。
    regexes: Option<crate::core::search::keyword::keyword_regex::RegexMatcherSet>,
    /// 去重后的关键词数（正则下标换算的基址）。
    keyword_count: usize,
    case_sensitive: bool,
    max_pattern_len: usize,
}

/// 生成一台自动机的 `(字节模式, 关键词下标)` 列表。模式按编码字节去重
/// （重复字节只留第一个，保持插入顺序）。
///
/// `folded` 时只收纯 ASCII 关键词，模式是 ASCII 折叠后的编码字节（匹配折叠
/// 缓冲区）；否则收全部关键词，大小写不敏感时对非 ASCII 关键词追加
/// `to_lowercase()` / `to_uppercase()` 变体（如「ÄPFEL」→「äpfel」；混合大小写
/// 不回退，保持字节精确）。
///
/// 大小写不敏感时，含字母的纯 ASCII 关键词不进常规自动机：ASCII 折叠是
/// 1:1 字节变换，折叠自动机（含该关键词的小写模式）覆盖原样命中且更全
/// （任意大小写组合），原样侧扫描纯粹是逐字节重复（区间去重会把重复
/// 命中丢掉，但扫描本身白跑）。无字母的纯 ASCII 关键词（如「123」）折叠是
/// 恒等变换、没有折叠模式，必须留在原样侧。
fn encode_patterns(
    deduped: &[String],
    encoding: SearchEncoding,
    case_sensitive: bool,
    folded: bool,
) -> (Vec<Vec<u8>>, Vec<usize>) {
    let mut patterns: Vec<Vec<u8>> = Vec::new();
    let mut pattern_keyword: Vec<usize> = Vec::new();
    let mut seen_patterns = HashSet::new();
    for (keyword_index, keyword) in deduped.iter().enumerate() {
        let mut variants: Vec<String> = Vec::new();
        if folded {
            // 只有纯 ASCII 关键词有折叠表达；无字母的关键词折叠是恒等变换，
            // 跳过（原样模式已由常规自动机覆盖）。
            if keyword.is_ascii() && keyword.bytes().any(|byte| byte.is_ascii_alphabetic()) {
                variants.push(keyword.to_ascii_lowercase());
            }
        } else {
            // 含字母的纯 ASCII 关键词归折叠自动机（见函数注释）；其余
            // （非 ASCII、无字母符号）留在原样侧。
            if !case_sensitive
                && keyword.is_ascii()
                && keyword.bytes().any(|byte| byte.is_ascii_alphabetic())
            {
                continue;
            }
            variants.push(keyword.clone());
            if !case_sensitive && !keyword.is_ascii() {
                variants.push(keyword.to_lowercase());
                variants.push(keyword.to_uppercase());
            }
        }
        for variant in variants {
            // 非 ASCII 变体在 ASCII 自动机里没有字节表达（encode 会拿 UTF-8
            // 字节顶替，产生错误命中），严格检查后跳过该变体。
            if encoding == SearchEncoding::Ascii && !variant.is_ascii() {
                continue;
            }
            let bytes = encoding.encode(&variant);
            if bytes.is_empty() {
                continue;
            }
            if seen_patterns.insert(bytes.clone()) {
                patterns.push(bytes);
                pattern_keyword.push(keyword_index);
            }
        }
    }
    (patterns, pattern_keyword)
}

impl EncodingMatcher {
    /// 构建单编码自动机；模式为空或构建失败（记录 warn）时返回 None。Standard
    /// 语义 + `find_overlapping_iter` 取全部重叠命中，「aa」在「aaa」中出 2 条。
    ///
    /// 优先构建 DFA（每字节一次表查，比默认 NFA 快数倍；模式集小时 DFA 只有
    /// 几十 KiB），DFA 确定化超出内部状态上限的病理模式集构建失败、回落 NFA。
    fn build(
        encoding: SearchEncoding,
        patterns: Vec<Vec<u8>>,
        pattern_keyword: Vec<usize>,
        max_pattern_len: &mut usize,
    ) -> Option<Self> {
        if patterns.is_empty() {
            return None;
        }
        let ac = match aho_corasick::AhoCorasickBuilder::new()
            .match_kind(aho_corasick::MatchKind::Standard)
            .kind(Some(aho_corasick::AhoCorasickKind::DFA))
            .build(&patterns)
            .or_else(|_| {
                aho_corasick::AhoCorasickBuilder::new()
                    .match_kind(aho_corasick::MatchKind::Standard)
                    .build(&patterns)
            }) {
            Ok(ac) => ac,
            Err(err) => {
                tracing::warn!("keyword_search: {encoding:?} matcher build failed: {err}");
                return None;
            }
        };
        *max_pattern_len = (*max_pattern_len).max(ac.max_pattern_len());
        Some(Self {
            encoding,
            ac,
            pattern_keyword,
        })
    }
}

impl MatcherSet {
    /// 全编码、无正则的兼容入口（等价 `build_plan(keywords, &[], …, ALL)`）。
    pub fn build(keywords: &[String], case_sensitive: bool) -> Self {
        let (set, failures) = build_matchers(keywords, &[], case_sensitive, &SearchEncoding::ALL);
        debug_assert!(failures.is_empty(), "no regexes → no compile failures");
        let _ = failures;
        set
    }

    /// 按选定编码页构建「关键词 + 正则」合并匹配器。
    /// 正则编译失败逐条汇总返回（用户输入错误，
    /// 由调用方拒绝整次搜索）；关键词自动机构建失败仅 warn 降级。
    pub fn build_plan(
        keywords: &[String],
        regexes: &[String],
        case_sensitive: bool,
        encodings: &[SearchEncoding],
    ) -> Result<Self, String> {
        let (set, failures) = build_matchers(keywords, regexes, case_sensitive, encodings);
        if failures.is_empty() {
            Ok(set)
        } else {
            Err(failures.join("；"))
        }
    }

    /// 去重后的关键词数；命中流的模式下标 `>= keyword_count` 即为正则
    /// （`index - keyword_count` 指回正则列表）。
    pub fn keyword_count(&self) -> usize {
        self.keyword_count
    }

    /// 所有编码自动机中最长的模式字节数；无模式时返回 0。
    pub fn max_pattern_len(&self) -> usize {
        self.max_pattern_len
    }

    /// 流式扫描的 carry 窗口大小：关键词侧取 `max_pattern_len - 1`，正则
    /// 通道取固定 [`REGEX_CARRY_BYTES`]（变长正则无最长模式上界）。
    pub fn carry_len(&self) -> usize {
        let literal = self.max_pattern_len.saturating_sub(1);
        if self.regexes.is_some() {
            literal.max(crate::core::search::keyword::keyword_regex::REGEX_CARRY_BYTES)
        } else {
            literal
        }
    }

    /// 扫描一个缓冲区（carry ∪ 新块）。`carry_len` 是 carry 字节数：
    /// 完全落在 carry 内的命中（start + len <= carry_len）是上一块的重复报告，
    /// 丢弃；跨越 carry / 新块边界的命中是新出现的，保留。跨编码按
    /// [`SearchEncoding::ALL`] 优先级对 `(start, len)` 去重。大小写不敏感时
    /// 把缓冲区 ASCII 折叠成一份副本，跑折叠自动机（纯 ASCII 关键词）。
    /// 正则通道（若有）在关键词之后追加，与关键词互不去重（同一区间被
    /// 关键词和正则同时命中时各报一条——它们是不同模式）。
    pub fn scan_chunk(&self, buffer: &[u8], carry_len: usize, out: &mut Vec<ByteHit>) {
        self.scan_chunk_with(buffer, carry_len, out, &mut MatchScratch::default());
    }

    /// [`Self::scan_chunk`]，折叠副本与去重集合放在调用方逐块复用的 `scratch`
    /// 里——流式扫描每块（小文件密集时即每个文件）都要用到它们。
    pub fn scan_chunk_with(
        &self,
        buffer: &[u8],
        carry_len: usize,
        out: &mut Vec<ByteHit>,
        scratch: &mut MatchScratch,
    ) {
        let MatchScratch {
            folded,
            seen_ranges,
        } = scratch;
        // clear() 要逐个重置桶：一次命中密集的块把集合撑大后，之后每个小文件都
        // 会付这笔开销，撑大了就直接换新。
        if seen_ranges.capacity() > MATCH_SCRATCH_RANGES_KEEP {
            *seen_ranges = HashSet::new();
        } else {
            seen_ranges.clear();
        }
        let folded_buffer = if self.case_sensitive || self.folded_matchers.is_empty() {
            None
        } else {
            // 折叠 = 拷贝 + 原地 SIMD 小写（make_ascii_lowercase），比逐字节
            // map+collect 快。
            folded.clear();
            folded.extend_from_slice(buffer);
            folded.make_ascii_lowercase();
            Some(folded.as_slice())
        };
        for encoding in SearchEncoding::ALL {
            let mut run = |matcher: &EncodingMatcher, haystack: &[u8]| {
                for hit in matcher.ac.find_overlapping_iter(haystack) {
                    let start = hit.start();
                    let len = hit.end() - start;
                    // carry 区域是上一块末尾，完全落入其中的命中上一块已报告过
                    // （模式最长 max_pattern_len，落在 carry 内的必在上块结尾之内）。
                    if start + len <= carry_len {
                        continue;
                    }
                    if !seen_ranges.insert((start, len)) {
                        continue;
                    }
                    out.push(ByteHit {
                        start,
                        len,
                        keyword_index: matcher.pattern_keyword[hit.pattern().as_usize()],
                        encoding,
                    });
                }
            };
            if let Some(matcher) = self.matchers.iter().find(|m| m.encoding == encoding) {
                run(matcher, buffer);
            }
            if let (Some(folded), Some(matcher)) = (
                folded_buffer,
                self.folded_matchers.iter().find(|m| m.encoding == encoding),
            ) {
                run(matcher, folded);
            }
        }
        if let Some(regexes) = &self.regexes {
            regexes.scan_chunk(buffer, carry_len, out);
        }
    }
}

/// 手动 Debug：自动机/正则内部结构无意义，打印聚合计数即可。
impl std::fmt::Debug for MatcherSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MatcherSet")
            .field("matchers", &self.matchers.len())
            .field("folded_matchers", &self.folded_matchers.len())
            .field("has_regexes", &self.regexes.is_some())
            .field("keyword_count", &self.keyword_count)
            .field("max_pattern_len", &self.max_pattern_len)
            .finish()
    }
}

/// `MatcherSet` 的实际构建体（`build`/`build_plan` 共用）。返回构建结果与
/// 正则编译失败列表。编码页按 [`SearchEncoding::ALL`] 优先级归一化去重；
/// 折叠自动机按「折叠模式集」去重（同一折叠字节集的编码只建第一台，等价
/// 于原实现跳过 UTF-8/GBK 折叠的优化，且对任意选定编码子集成立）。
fn build_matchers(
    keywords: &[String],
    regexes: &[String],
    case_sensitive: bool,
    encodings: &[SearchEncoding],
) -> (MatcherSet, Vec<String>) {
    let mut deduped: Vec<String> = Vec::new();
    let mut seen_keywords = HashSet::new();
    for keyword in keywords {
        if keyword.trim().is_empty() {
            continue;
        }
        if seen_keywords.insert(keyword.clone()) {
            deduped.push(keyword.clone());
        }
    }
    let mut deduped_regexes: Vec<String> = Vec::new();
    let mut seen_regexes = HashSet::new();
    for pattern in regexes {
        if pattern.trim().is_empty() {
            continue;
        }
        if seen_regexes.insert(pattern.clone()) {
            deduped_regexes.push(pattern.clone());
        }
    }
    let encodings = normalize_encodings(encodings);

    let mut matchers = Vec::new();
    let mut folded_matchers = Vec::new();
    let mut max_pattern_len = 0;
    // 已构建自动机的字节模式集：不同编码的模式集完全相同时（纯 ASCII /
    // 无字母关键词在 ASCII / UTF-8 / GBK 下字节相同），只建第一台——命中
    // 区间去重本就把后续编码的重复命中丢弃，重复扫描纯属浪费，且第一台
    // （高优先级编码）已覆盖。
    let mut built_pattern_sets: Vec<Vec<Vec<u8>>> = Vec::new();
    let mut built_folded_pattern_sets: Vec<Vec<Vec<u8>>> = Vec::new();
    for encoding in &encodings {
        let (patterns, pattern_keyword) =
            encode_patterns(&deduped, *encoding, case_sensitive, false);
        if !built_pattern_sets.contains(&patterns) {
            if let Some(matcher) = EncodingMatcher::build(
                *encoding,
                patterns.clone(),
                pattern_keyword,
                &mut max_pattern_len,
            ) {
                matchers.push(matcher);
                built_pattern_sets.push(patterns);
            }
        }
        if !case_sensitive {
            let (patterns, pattern_keyword) =
                encode_patterns(&deduped, *encoding, case_sensitive, true);
            if !patterns.is_empty() && !built_folded_pattern_sets.contains(&patterns) {
                if let Some(matcher) = EncodingMatcher::build(
                    *encoding,
                    patterns.clone(),
                    pattern_keyword,
                    &mut max_pattern_len,
                ) {
                    folded_matchers.push(matcher);
                    built_folded_pattern_sets.push(patterns);
                }
            }
        }
    }
    let regex_matchers = if deduped_regexes.is_empty() {
        None
    } else {
        let (set, failures) = crate::core::search::keyword::keyword_regex::RegexMatcherSet::build(
            &deduped_regexes,
            case_sensitive,
            &encodings,
            deduped.len(),
        );
        if !failures.is_empty() {
            return (empty_matcher_set(), failures);
        }
        if set.is_empty() {
            // 选定编码下没有任何可表达的正则（如仅选 ASCII 却全是非 ASCII 模式）。
            tracing::warn!("keyword_search: regex matchers empty for selected encodings");
            None
        } else {
            Some(set)
        }
    };
    let keyword_count = deduped.len();
    (
        MatcherSet {
            matchers,
            folded_matchers,
            regexes: regex_matchers,
            keyword_count,
            case_sensitive,
            max_pattern_len,
        },
        Vec::new(),
    )
}

/// 空匹配器（所有正则编译失败时的占位，调用方拿到 Err 后不会使用它）。
fn empty_matcher_set() -> MatcherSet {
    MatcherSet {
        matchers: Vec::new(),
        folded_matchers: Vec::new(),
        regexes: None,
        keyword_count: 0,
        case_sensitive: true,
        max_pattern_len: 0,
    }
}

/// 编码页列表按 [`SearchEncoding::ALL`] 优先级排序并去重（跨编码区间去重
/// 依赖该优先级；空列表视为全部编码）。
fn normalize_encodings(encodings: &[SearchEncoding]) -> Vec<SearchEncoding> {
    if encodings.is_empty() {
        return SearchEncoding::ALL.to_vec();
    }
    SearchEncoding::ALL
        .iter()
        .copied()
        .filter(|encoding| encodings.contains(encoding))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_ascii_keyword_matches_any_case() {
        let matcher = MatcherSet::build(&["password".to_string()], false);
        // UTF-16 变体最长：8 字符 × 2 字节
        assert_eq!(matcher.max_pattern_len(), 16);

        let mut hits = Vec::new();
        matcher.scan_chunk(b"login: Password ok", 0, &mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start, 7);
        assert_eq!(hits[0].len, 8);
        assert_eq!(hits[0].keyword_index, 0);
        assert_eq!(hits[0].encoding, SearchEncoding::Ascii);

        hits.clear();
        matcher.scan_chunk(b"PASSWORD", 0, &mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].encoding, SearchEncoding::Ascii);
    }

    #[test]
    fn gbk_keyword_hits_gbk_bytes() {
        let pattern = SearchEncoding::Gbk.encode("密码");
        assert_eq!(pattern.len(), 4);

        let matcher = MatcherSet::build(&["密码".to_string()], true);
        let mut sample = Vec::new();
        sample.extend_from_slice(b"abc");
        sample.extend_from_slice(&pattern);
        sample.extend_from_slice(b"def");
        let mut hits = Vec::new();
        matcher.scan_chunk(&sample, 0, &mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start, 3);
        assert_eq!(hits[0].len, 4);
        assert_eq!(hits[0].encoding, SearchEncoding::Gbk);
        assert_eq!(hits[0].keyword_index, 0);
    }

    #[test]
    fn utf16le_keyword_hits_interleaved_bytes() {
        let matcher = MatcherSet::build(&["password".to_string()], false);
        let mut sample = Vec::new();
        for unit in "Password".encode_utf16() {
            sample.extend_from_slice(&unit.to_le_bytes());
        }
        let mut hits = Vec::new();
        matcher.scan_chunk(&sample, 0, &mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start, 0);
        assert_eq!(hits[0].len, "password".len() * 2);
        assert_eq!(hits[0].encoding, SearchEncoding::Utf16Le);
    }

    #[test]
    fn keyword_spanning_chunk_boundary_reported_once() {
        // 「password」跨 chunk1/chunk2 边界；carry 内还有一段 x*9 验证完全落入
        // carry 的重复命中（"xx"）被丢弃、不重复报告。
        let matcher = MatcherSet::build(&["password".to_string(), "xx".to_string()], true);
        let carry_len = matcher.max_pattern_len() - 1;

        let mut chunk1 = vec![b'x'; 100];
        chunk1.extend_from_slice(b"passwo");
        let mut chunk2 = vec![b'r', b'd'];
        chunk2.extend(std::iter::repeat(b'y').take(100));

        // 第一块：carry 为空
        let mut hits = Vec::new();
        matcher.scan_chunk(&chunk1, 0, &mut hits);
        // x*100 中 "xx" 重叠命中 99 条，无跨块信息
        assert_eq!(hits.len(), 99);
        assert!(hits.iter().all(|hit| hit.keyword_index == 1));

        // 第二块：carry = 上一块末尾 carry_len 字节
        let carry = &chunk1[chunk1.len() - carry_len..];
        let mut buffer = Vec::with_capacity(carry.len() + chunk2.len());
        buffer.extend_from_slice(carry);
        buffer.extend_from_slice(&chunk2);

        hits.clear();
        matcher.scan_chunk(&buffer, carry.len(), &mut hits);
        // 只剩跨边界的 "password"："xx" 命中全部完全落入 carry，被丢弃
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start, carry.len() - 6);
        assert_eq!(hits[0].len, 8);
        assert_eq!(hits[0].keyword_index, 0);
        assert_eq!(hits[0].encoding, SearchEncoding::Ascii);
    }

    #[test]
    fn overlapping_matches_all_reported() {
        let matcher = MatcherSet::build(&["aa".to_string()], true);
        let mut hits = Vec::new();
        matcher.scan_chunk(b"aaa", 0, &mut hits);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|hit| hit.start == 0 && hit.len == 2));
        assert!(hits.iter().any(|hit| hit.start == 1 && hit.len == 2));
        assert!(hits.iter().all(|hit| hit.encoding == SearchEncoding::Ascii));
    }

    #[test]
    fn ascii_utf8_duplicate_reported_once_as_ascii() {
        // ASCII 与 UTF-8 自动机命中同一字节区间，只报 1 条、标记为 ASCII
        let matcher = MatcherSet::build(&["password".to_string()], true);
        let mut hits = Vec::new();
        matcher.scan_chunk(b"password", 0, &mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start, 0);
        assert_eq!(hits[0].len, 8);
        assert_eq!(hits[0].encoding, SearchEncoding::Ascii);
    }

    #[test]
    fn empty_keyword_list_scans_nothing() {
        let matcher = MatcherSet::build(&[], true);
        assert_eq!(matcher.max_pattern_len(), 0);
        let mut hits = Vec::new();
        matcher.scan_chunk(b"anything at all", 0, &mut hits);
        assert!(hits.is_empty());

        let blank = MatcherSet::build(&["  ".to_string()], true);
        let mut hits = Vec::new();
        blank.scan_chunk(b"", 0, &mut hits);
        assert!(hits.is_empty());
    }

    #[test]
    fn utf16_round_trips_both_endiannesses() {
        let text = "Hello 世界";
        let be = SearchEncoding::Utf16Be.encode(text);
        let le = SearchEncoding::Utf16Le.encode(text);
        assert_eq!(SearchEncoding::Utf16Be.decode_to_string(&be), text);
        assert_eq!(SearchEncoding::Utf16Le.decode_to_string(&le), text);
        assert_ne!(be, le);
    }

    #[test]
    fn gbk_round_trips() {
        let text = "密码测试";
        let bytes = SearchEncoding::Gbk.encode(text);
        assert_eq!(SearchEncoding::Gbk.decode_to_string(&bytes), text);
    }

    #[test]
    fn decode_tolerates_partial_multibyte_tail() {
        // 半个 UTF-8 字符的尾巴以替换字符解码，不 panic
        let decoded = SearchEncoding::Utf8.decode_to_string(&[b'a', 0xE5]);
        assert!(decoded.starts_with('a'));
        assert!(decoded.contains('\u{FFFD}'));
    }

    #[test]
    fn labels_cover_all_encodings() {
        assert_eq!(SearchEncoding::ALL.len(), 5);
        assert_eq!(SearchEncoding::Ascii.label(), "ASCII");
        assert_eq!(SearchEncoding::Utf8.label(), "UTF-8");
        assert_eq!(SearchEncoding::Utf16Le.label(), "UTF-16LE");
        assert_eq!(SearchEncoding::Utf16Be.label(), "UTF-16BE");
        assert_eq!(SearchEncoding::Gbk.label(), "GBK");
    }

    #[test]
    fn keyword_indices_refer_to_deduped_list() {
        // "alpha" 重复出现只算一条；"beta" 是第 2 条
        let matcher = MatcherSet::build(
            &["alpha".to_string(), "alpha".to_string(), "beta".to_string()],
            true,
        );
        let mut hits = Vec::new();
        matcher.scan_chunk(b"beta alpha", 0, &mut hits);
        assert_eq!(hits.len(), 2);
        hits.sort_by_key(|hit| hit.start);
        assert_eq!(hits[0].start, 0);
        assert_eq!(hits[0].keyword_index, 1);
        assert_eq!(hits[1].start, 5);
        assert_eq!(hits[1].keyword_index, 0);
    }

    #[test]
    fn from_label_parses_codepages() {
        assert_eq!(
            SearchEncoding::from_label("UTF-8"),
            Some(SearchEncoding::Utf8)
        );
        assert_eq!(
            SearchEncoding::from_label("utf-16le"),
            Some(SearchEncoding::Utf16Le)
        );
        assert_eq!(
            SearchEncoding::from_label(" GBK "),
            Some(SearchEncoding::Gbk)
        );
        assert_eq!(SearchEncoding::from_label("latin1"), None);
    }

    #[test]
    fn selected_encodings_restrict_matchers() {
        // 仅选 GBK：UTF-16LE 字节流不再命中
        let matcher =
            MatcherSet::build_plan(&["密码".to_string()], &[], true, &[SearchEncoding::Gbk])
                .expect("build plan");
        assert_eq!(matcher.keyword_count(), 1);
        let utf16 = SearchEncoding::Utf16Le.encode("密码");
        let mut hits = Vec::new();
        matcher.scan_chunk(&utf16, 0, &mut hits);
        assert!(hits.is_empty());

        let gbk = SearchEncoding::Gbk.encode("密码");
        matcher.scan_chunk(&gbk, 0, &mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].encoding, SearchEncoding::Gbk);
    }

    #[test]
    fn plan_merges_keyword_and_regex_hits() {
        let matcher = MatcherSet::build_plan(
            &["password".to_string()],
            &[r"sekret\d+".to_string()],
            true,
            &[SearchEncoding::Ascii],
        )
        .expect("build plan");
        assert_eq!(matcher.keyword_count(), 1);
        // 正则通道启用后 carry 窗口取正则固定值
        assert_eq!(
            matcher.carry_len(),
            crate::core::search::keyword::keyword_regex::REGEX_CARRY_BYTES
        );

        let mut hits = Vec::new();
        matcher.scan_chunk(b"password sekret7 ok", 0, &mut hits);
        hits.sort_by_key(|hit| hit.start);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].keyword_index, 0); // password（关键词）
        assert_eq!(hits[1].keyword_index, 1); // sekret7（正则，基址=关键词数）
    }

    #[test]
    fn invalid_regex_rejects_whole_plan() {
        let err = MatcherSet::build_plan(
            &["ok".to_string()],
            &["[bad".to_string()],
            true,
            &SearchEncoding::ALL.to_vec(),
        )
        .expect_err("must fail");
        assert!(err.contains("[bad"));
    }

    #[test]
    fn carry_window_without_regex_is_pattern_bound() {
        let matcher = MatcherSet::build(&["password".to_string()], true);
        assert_eq!(matcher.carry_len(), matcher.max_pattern_len() - 1);
    }
}
