use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::syntax::escape_html;

/// Maximum size of a file the binary engine will read entirely into memory for
/// content comparison. Larger files are rejected to prevent OOM.
const MAX_BINARY_CONTENT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypedValueKind {
    U8,
    I8,
    U16Le,
    U16Be,
    I16Le,
    I16Be,
    U32Le,
    U32Be,
    I32Le,
    I32Be,
    U64Le,
    U64Be,
    I64Le,
    I64Be,
    F32Le,
    F32Be,
    F64Le,
    F64Be,
}

impl TypedValueKind {
    pub fn byte_count(self) -> usize {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16Le | Self::U16Be | Self::I16Le | Self::I16Be => 2,
            Self::U32Le | Self::U32Be | Self::I32Le | Self::I32Be | Self::F32Le | Self::F32Be => 4,
            Self::U64Le | Self::U64Be | Self::I64Le | Self::I64Be | Self::F64Le | Self::F64Be => 8,
        }
    }

    pub fn format_value(self, bytes: &[u8]) -> String {
        match self {
            Self::U8 => bytes[0].to_string(),
            Self::I8 => (bytes[0] as i8).to_string(),
            Self::U16Le => u16::from_le_bytes([bytes[0], bytes[1]]).to_string(),
            Self::U16Be => u16::from_be_bytes([bytes[0], bytes[1]]).to_string(),
            Self::I16Le => i16::from_le_bytes([bytes[0], bytes[1]]).to_string(),
            Self::I16Be => i16::from_be_bytes([bytes[0], bytes[1]]).to_string(),
            Self::U32Le => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).to_string(),
            Self::U32Be => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).to_string(),
            Self::I32Le => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).to_string(),
            Self::I32Be => i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).to_string(),
            Self::U64Le => u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ])
            .to_string(),
            Self::U64Be => u64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ])
            .to_string(),
            Self::I64Le => i64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ])
            .to_string(),
            Self::I64Be => i64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ])
            .to_string(),
            Self::F32Le => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).to_string(),
            Self::F32Be => f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).to_string(),
            Self::F64Le => f64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ])
            .to_string(),
            Self::F64Be => f64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ])
            .to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypedInterpretation {
    pub offset: usize,
    pub kind: TypedValueKind,
    pub left_value: String,
    pub right_value: String,
    pub differs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BinaryCompareOptions {
    pub bytes_per_row: usize,
    pub compare_content: bool,
    pub compare_metadata: bool,
}

impl Default for BinaryCompareOptions {
    fn default() -> Self {
        Self {
            bytes_per_row: 16,
            compare_content: true,
            compare_metadata: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    pub offset: usize,
    pub side: SearchSide,
    pub length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSide {
    Left,
    Right,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HexParseError {
    InvalidLength,
    InvalidCharacter(char),
}

impl std::fmt::Display for HexParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HexParseError::InvalidLength => {
                write!(f, "hex string must have an even number of hex digits")
            }
            HexParseError::InvalidCharacter(c) => write!(f, "invalid hex character: '{c}'"),
        }
    }
}

impl std::error::Error for HexParseError {}

pub fn parse_hex_pattern(hex: &str) -> Result<Vec<u8>, HexParseError> {
    let cleaned: Vec<char> = hex.split_whitespace().flat_map(str::chars).collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    if !cleaned.len().is_multiple_of(2) {
        return Err(HexParseError::InvalidLength);
    }
    let mut bytes = Vec::with_capacity(cleaned.len() / 2);
    for chunk in cleaned.chunks(2) {
        let high = hex_digit_value(chunk[0]).ok_or(HexParseError::InvalidCharacter(chunk[0]))?;
        let low = hex_digit_value(chunk[1]).ok_or(HexParseError::InvalidCharacter(chunk[1]))?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_digit_value(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        'a'..='f' => Some(c as u8 - b'a' + 10),
        'A'..='F' => Some(c as u8 - b'A' + 10),
        _ => None,
    }
}

fn find_all(haystack: &[u8], needle: &[u8], from_offset: usize) -> Vec<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return Vec::new();
    }
    let start = from_offset.min(haystack.len());
    let mut offsets = Vec::new();
    let mut pos = start;
    let end = haystack.len() - needle.len() + 1;
    while pos < end {
        if haystack[pos..].starts_with(needle) {
            offsets.push(pos);
        }
        pos += 1;
    }
    offsets
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryCompareResult {
    pub left_name: String,
    pub right_name: String,
    pub left_len: usize,
    pub right_len: usize,
    pub content_compared: bool,
    pub metadata: Option<BinaryMetadataCompare>,
    pub metadata_differences: Vec<BinaryMetadataDifference>,
    pub differences: Vec<ByteDiff>,
    pub rows: Vec<HexRow>,
    // Raw buffers back search/paging but are not part of the serialized report:
    // skipped so a saved result stays small and re-renders from `rows`.
    #[serde(skip)]
    pub(crate) left_data: Vec<u8>,
    #[serde(skip)]
    pub(crate) right_data: Vec<u8>,
}

impl BinaryCompareResult {
    pub fn is_equal(&self) -> bool {
        self.differences.is_empty() && self.metadata_differences.is_empty()
    }

    /// Render a self-contained HTML hex report from the computed rows, with
    /// differing rows highlighted. Renders from `rows`, so it works on a result
    /// re-loaded from JSON (where the raw buffers are not present).
    pub fn to_html_report(&self) -> String {
        let mut html = String::new();
        html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">\n");
        html.push_str(&format!(
            "<title>LinSync binary report: {} vs {}</title>\n",
            escape_html(&self.left_name),
            escape_html(&self.right_name)
        ));
        html.push_str(
            "<style>\n\
             body{font-family:system-ui,sans-serif;margin:1.5rem;}\n\
             table{border-collapse:collapse;}\n\
             td,th{border:1px solid #ccc;padding:2px 6px;font-family:monospace;white-space:pre;}\n\
             tr.diff{background:#fff3b0;}\n\
             td.off{color:#888;}\n\
             </style>\n</head><body>\n",
        );
        html.push_str(&format!(
            "<h1>{} vs {}</h1>\n",
            escape_html(&self.left_name),
            escape_html(&self.right_name)
        ));
        html.push_str(&format!(
            "<p>left {} bytes, right {} bytes; {} differing byte(s)",
            self.left_len,
            self.right_len,
            self.differences.len()
        ));
        if !self.metadata_differences.is_empty() {
            let parts: Vec<&str> = self
                .metadata_differences
                .iter()
                .map(|d| d.as_str())
                .collect();
            html.push_str(&format!("; metadata differs: {}", parts.join(", ")));
        }
        html.push_str(".</p>\n");
        if !self.content_compared {
            html.push_str("<p>(metadata-only comparison)</p>\n");
        }
        html.push_str(
            "<table>\n<thead><tr><th>offset</th><th>left (hex)</th><th>left (ascii)</th>\
             <th>right (hex)</th><th>right (ascii)</th></tr></thead>\n<tbody>\n",
        );
        for row in &self.rows {
            let class = if row.has_difference {
                " class=\"diff\""
            } else {
                ""
            };
            html.push_str(&format!(
                "<tr{class}><td class=\"off\">{:08x}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                row.offset,
                escape_html(&row.left_hex),
                escape_html(&row.left_ascii),
                escape_html(&row.right_hex),
                escape_html(&row.right_ascii),
            ));
        }
        html.push_str("</tbody></table>\n</body></html>\n");
        html
    }

    pub fn search_bytes(&self, pattern: &[u8], from_offset: usize) -> Vec<SearchMatch> {
        if pattern.is_empty() {
            return Vec::new();
        }
        let left_offsets = find_all(&self.left_data, pattern, from_offset);
        let right_offsets = find_all(&self.right_data, pattern, from_offset);
        let mut matches = Vec::new();
        let mut li = 0;
        let mut ri = 0;
        while li < left_offsets.len() || ri < right_offsets.len() {
            let lo = left_offsets.get(li).copied();
            let ro = right_offsets.get(ri).copied();
            match (lo, ro) {
                (Some(l), Some(r)) if l == r => {
                    matches.push(SearchMatch {
                        offset: l,
                        side: SearchSide::Both,
                        length: pattern.len(),
                    });
                    li += 1;
                    ri += 1;
                }
                (Some(l), Some(r)) if l < r => {
                    matches.push(SearchMatch {
                        offset: l,
                        side: SearchSide::Left,
                        length: pattern.len(),
                    });
                    li += 1;
                }
                (Some(_), Some(r)) => {
                    matches.push(SearchMatch {
                        offset: r,
                        side: SearchSide::Right,
                        length: pattern.len(),
                    });
                    ri += 1;
                }
                (Some(l), None) => {
                    matches.push(SearchMatch {
                        offset: l,
                        side: SearchSide::Left,
                        length: pattern.len(),
                    });
                    li += 1;
                }
                (None, Some(r)) => {
                    matches.push(SearchMatch {
                        offset: r,
                        side: SearchSide::Right,
                        length: pattern.len(),
                    });
                    ri += 1;
                }
                (None, None) => break,
            }
        }
        matches
    }

    pub fn search_text(&self, text: &str, from_offset: usize) -> Vec<SearchMatch> {
        self.search_bytes(text.as_bytes(), from_offset)
    }

    pub fn search_hex(
        &self,
        hex_pattern: &str,
        from_offset: usize,
    ) -> Result<Vec<SearchMatch>, HexParseError> {
        let bytes = parse_hex_pattern(hex_pattern)?;
        Ok(self.search_bytes(&bytes, from_offset))
    }

    pub fn first_difference(&self) -> Option<usize> {
        self.differences.first().map(|d| d.offset)
    }

    pub fn next_difference_after(&self, offset: usize) -> Option<usize> {
        self.differences
            .iter()
            .find(|d| d.offset > offset)
            .map(|d| d.offset)
    }

    pub fn previous_difference_before(&self, offset: usize) -> Option<usize> {
        self.differences
            .iter()
            .rev()
            .find(|d| d.offset < offset)
            .map(|d| d.offset)
    }

    pub fn difference_at(&self, offset: usize) -> bool {
        self.differences.iter().any(|d| d.offset == offset)
    }

    pub fn hex_page(&self, page: usize, page_size: usize) -> HexPage {
        // Saturating arithmetic: large page/page_size values would otherwise
        // panic on overflow in debug builds and wrap in release, slipping a
        // bogus `start` past the bounds check below.
        let start = page.saturating_mul(page_size);
        let rows = if start < self.rows.len() {
            let end = start.saturating_add(page_size).min(self.rows.len());
            self.rows[start..end].to_vec()
        } else {
            Vec::new()
        };
        let start_offset = rows.first().map(|r| r.offset).unwrap_or(0);
        HexPage {
            start_offset,
            rows,
            total_rows: self.rows.len(),
            page,
            page_size,
        }
    }

    pub fn interpret_at(&self, offset: usize, kind: TypedValueKind) -> Option<TypedInterpretation> {
        let byte_count = kind.byte_count();
        let left_slice = self
            .left_data
            .get(offset..offset.checked_add(byte_count)?)?;
        let right_slice = self
            .right_data
            .get(offset..offset.checked_add(byte_count)?)?;
        let left_value = kind.format_value(left_slice);
        let right_value = kind.format_value(right_slice);
        let differs = left_slice != right_slice;
        Some(TypedInterpretation {
            offset,
            kind,
            left_value,
            right_value,
            differs,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryMetadataCompare {
    pub left: BinaryFileMetadata,
    pub right: BinaryFileMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryFileMetadata {
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub readonly: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryMetadataDifference {
    Size,
    Modified,
    ReadOnly,
}

impl BinaryMetadataDifference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Modified => "modified",
            Self::ReadOnly => "readonly",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteDiff {
    pub offset: usize,
    pub left: Option<u8>,
    pub right: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HexRow {
    pub offset: usize,
    pub left_hex: String,
    pub right_hex: String,
    pub left_ascii: String,
    pub right_ascii: String,
    pub has_difference: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexPage {
    pub start_offset: usize,
    pub rows: Vec<HexRow>,
    pub total_rows: usize,
    pub page: usize,
    pub page_size: usize,
}

pub fn compare_binary_files(
    left: &Path,
    right: &Path,
    options: &BinaryCompareOptions,
) -> io::Result<BinaryCompareResult> {
    let metadata = if options.compare_metadata {
        let left_metadata = BinaryFileMetadata::from_path(left)?;
        let right_metadata = BinaryFileMetadata::from_path(right)?;
        Some(BinaryMetadataCompare {
            left: left_metadata,
            right: right_metadata,
        })
    } else {
        None
    };

    let (left_bytes, right_bytes) = if options.compare_content {
        for path in [left, right] {
            let len = fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX);
            if len > MAX_BINARY_CONTENT_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "file size {len} exceeds {MAX_BINARY_CONTENT_BYTES} byte binary-content limit"
                    ),
                ));
            }
        }
        (fs::read(left)?, fs::read(right)?)
    } else {
        (Vec::new(), Vec::new())
    };

    Ok(compare_binary_with_metadata(
        &left.display().to_string(),
        &left_bytes,
        &right.display().to_string(),
        &right_bytes,
        options,
        metadata,
    ))
}

pub fn compare_binary(
    left_name: &str,
    left: &[u8],
    right_name: &str,
    right: &[u8],
    options: &BinaryCompareOptions,
) -> BinaryCompareResult {
    compare_binary_with_metadata(left_name, left, right_name, right, options, None)
}

fn compare_binary_with_metadata(
    left_name: &str,
    left: &[u8],
    right_name: &str,
    right: &[u8],
    options: &BinaryCompareOptions,
    metadata: Option<BinaryMetadataCompare>,
) -> BinaryCompareResult {
    let bytes_per_row = options.bytes_per_row.max(1);
    let metadata_differences = metadata
        .as_ref()
        .map(metadata_differences)
        .unwrap_or_default();
    let (differences, rows) = if options.compare_content {
        content_differences(left, right, bytes_per_row)
    } else {
        (Vec::new(), Vec::new())
    };

    BinaryCompareResult {
        left_name: left_name.to_owned(),
        right_name: right_name.to_owned(),
        left_len: metadata
            .as_ref()
            .map(|metadata| metadata.left.len as usize)
            .unwrap_or(left.len()),
        right_len: metadata
            .as_ref()
            .map(|metadata| metadata.right.len as usize)
            .unwrap_or(right.len()),
        content_compared: options.compare_content,
        metadata,
        metadata_differences,
        differences,
        rows,
        left_data: left.to_vec(),
        right_data: right.to_vec(),
    }
}

impl BinaryFileMetadata {
    fn from_path(path: &Path) -> io::Result<Self> {
        let metadata = fs::metadata(path)?;
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            readonly: metadata.permissions().readonly(),
        })
    }
}

fn metadata_differences(metadata: &BinaryMetadataCompare) -> Vec<BinaryMetadataDifference> {
    let mut differences = Vec::new();
    if metadata.left.len != metadata.right.len {
        differences.push(BinaryMetadataDifference::Size);
    }
    if metadata.left.modified != metadata.right.modified {
        differences.push(BinaryMetadataDifference::Modified);
    }
    if metadata.left.readonly != metadata.right.readonly {
        differences.push(BinaryMetadataDifference::ReadOnly);
    }
    differences
}

fn content_differences(
    left: &[u8],
    right: &[u8],
    bytes_per_row: usize,
) -> (Vec<ByteDiff>, Vec<HexRow>) {
    let max_len = left.len().max(right.len());
    let mut differences = Vec::new();
    for offset in 0..max_len {
        let left_byte = left.get(offset).copied();
        let right_byte = right.get(offset).copied();
        if left_byte != right_byte {
            differences.push(ByteDiff {
                offset,
                left: left_byte,
                right: right_byte,
            });
        }
    }

    let rows = generate_hex_rows(left, right, bytes_per_row);
    (differences, rows)
}

pub fn generate_hex_rows(left: &[u8], right: &[u8], bytes_per_row: usize) -> Vec<HexRow> {
    let bytes_per_row = bytes_per_row.max(1);
    let max_len = left.len().max(right.len());
    let mut rows = Vec::new();
    let mut offset = 0;
    while offset < max_len {
        let left_end = (offset + bytes_per_row).min(left.len());
        let right_end = (offset + bytes_per_row).min(right.len());
        let left_chunk = if offset < left.len() {
            &left[offset..left_end]
        } else {
            &[]
        };
        let right_chunk = if offset < right.len() {
            &right[offset..right_end]
        } else {
            &[]
        };

        let has_difference = left_chunk != right_chunk;

        rows.push(HexRow {
            offset,
            left_hex: hex_bytes(left_chunk),
            right_hex: hex_bytes(right_chunk),
            left_ascii: ascii_preview(left_chunk),
            right_ascii: ascii_preview(right_chunk),
            has_difference,
        });
        offset += bytes_per_row;
    }
    rows
}

pub fn is_likely_binary(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return true;
    }

    let sample_len = bytes.len().min(4096);
    if sample_len == 0 {
        return false;
    }

    let control_count = bytes[..sample_len]
        .iter()
        .filter(|byte| byte.is_ascii_control() && !matches!(byte, b'\n' | b'\r' | b'\t'))
        .count();
    control_count * 100 / sample_len > 10
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn ascii_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '.'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_equal_binary() {
        let result = compare_binary(
            "left",
            b"\x00abc",
            "right",
            b"\x00abc",
            &BinaryCompareOptions::default(),
        );

        assert!(result.is_equal());
        assert_eq!(result.rows[0].left_hex, "00 61 62 63");
        assert_eq!(result.rows[0].left_ascii, ".abc");
    }

    #[test]
    fn reports_changed_and_missing_bytes() {
        let result = compare_binary(
            "left",
            b"abcd",
            "right",
            b"abXYz",
            &BinaryCompareOptions {
                bytes_per_row: 4,
                ..BinaryCompareOptions::default()
            },
        );

        assert!(!result.is_equal());
        assert_eq!(
            result.differences,
            vec![
                ByteDiff {
                    offset: 2,
                    left: Some(b'c'),
                    right: Some(b'X'),
                },
                ByteDiff {
                    offset: 3,
                    left: Some(b'd'),
                    right: Some(b'Y'),
                },
                ByteDiff {
                    offset: 4,
                    left: None,
                    right: Some(b'z'),
                },
            ]
        );
        assert!(result.rows[0].has_difference);
        assert!(result.rows[1].has_difference);
    }

    #[test]
    fn detects_binary_bytes() {
        assert!(is_likely_binary(b"hello\0world"));
        assert!(!is_likely_binary(b"hello\nworld\n"));
    }

    #[test]
    fn can_compare_file_metadata_without_reading_content() {
        let root = std::env::temp_dir().join(format!(
            "linsync-binary-metadata-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let left = root.join("left.bin");
        let right = root.join("right.bin");
        fs::write(&left, b"\0left").unwrap();
        fs::write(&right, b"\0right-side").unwrap();

        let result = compare_binary_files(
            &left,
            &right,
            &BinaryCompareOptions {
                compare_content: false,
                compare_metadata: true,
                ..BinaryCompareOptions::default()
            },
        )
        .unwrap();

        assert!(!result.is_equal());
        assert!(!result.content_compared);
        assert!(result.differences.is_empty());
        assert!(result.rows.is_empty());
        assert!(
            result
                .metadata_differences
                .contains(&BinaryMetadataDifference::Size)
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_bytes_finds_pattern() {
        let result = compare_binary(
            "left",
            b"\x00\x01\x02\x03\x01\x02\x04",
            "right",
            b"\x00\x01\x02\x03\x01\x02\x04",
            &BinaryCompareOptions::default(),
        );
        let matches = result.search_bytes(&[0x01, 0x02], 0);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].offset, 1);
        assert_eq!(matches[0].side, SearchSide::Both);
        assert_eq!(matches[1].offset, 4);
        assert_eq!(matches[1].side, SearchSide::Both);
    }

    #[test]
    fn search_text_finds_utf8() {
        let result = compare_binary(
            "left",
            b"Hello World Hello",
            "right",
            b"Hello World Hello",
            &BinaryCompareOptions::default(),
        );
        let matches = result.search_text("Hello", 0);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].offset, 0);
        assert_eq!(matches[0].side, SearchSide::Both);
        assert_eq!(matches[0].length, 5);
        assert_eq!(matches[1].offset, 12);
        assert_eq!(matches[1].side, SearchSide::Both);
    }

    #[test]
    fn search_hex_parses_and_finds() {
        let result = compare_binary(
            "left",
            b"HELLO",
            "right",
            b"HELLO",
            &BinaryCompareOptions::default(),
        );
        let matches = result.search_hex("48454C4C4F", 0).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset, 0);
        assert_eq!(matches[0].side, SearchSide::Both);
        assert_eq!(matches[0].length, 5);
    }

    #[test]
    fn search_hex_rejects_invalid() {
        let result = compare_binary(
            "left",
            b"\x00",
            "right",
            b"\x00",
            &BinaryCompareOptions::default(),
        );
        match result.search_hex("ZZ", 0) {
            Err(HexParseError::InvalidCharacter('Z')) => {}
            other => panic!("expected InvalidCharacter('Z'), got {other:?}"),
        }
        match result.search_hex("ABC", 0) {
            Err(HexParseError::InvalidLength) => {}
            other => panic!("expected InvalidLength, got {other:?}"),
        }
    }

    #[test]
    fn search_returns_empty_for_no_match() {
        let result = compare_binary(
            "left",
            b"abcdef",
            "right",
            b"abcdef",
            &BinaryCompareOptions::default(),
        );
        assert!(result.search_bytes(b"xyz", 0).is_empty());
    }

    #[test]
    fn search_from_offset_skips_early_matches() {
        let result = compare_binary(
            "left",
            b"abababab",
            "right",
            b"abababab",
            &BinaryCompareOptions::default(),
        );
        assert_eq!(result.search_bytes(b"ab", 0).len(), 4);
        let offset_matches = result.search_bytes(b"ab", 4);
        assert!(offset_matches.iter().all(|m| m.offset >= 4));
        assert_eq!(offset_matches.len(), 2);
    }

    #[test]
    fn search_bytes_left_only_match() {
        let result = compare_binary(
            "left",
            b"abcXYZdef",
            "right",
            b"abc___def",
            &BinaryCompareOptions::default(),
        );
        let matches = result.search_bytes(b"XYZ", 0);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].side, SearchSide::Left);
        assert_eq!(matches[0].offset, 3);
    }

    #[test]
    fn search_bytes_right_only_match() {
        let result = compare_binary(
            "left",
            b"abc___def",
            "right",
            b"abcXYZdef",
            &BinaryCompareOptions::default(),
        );
        let matches = result.search_bytes(b"XYZ", 0);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].side, SearchSide::Right);
        assert_eq!(matches[0].offset, 3);
    }

    #[test]
    fn search_empty_pattern_returns_empty() {
        let result = compare_binary(
            "left",
            b"data",
            "right",
            b"data",
            &BinaryCompareOptions::default(),
        );
        assert!(result.search_bytes(&[], 0).is_empty());
    }

    #[test]
    fn parse_hex_pattern_roundtrip() {
        assert_eq!(
            parse_hex_pattern("FF 0a 1B").unwrap(),
            vec![0xFF, 0x0A, 0x1B]
        );
        assert!(parse_hex_pattern("").unwrap().is_empty());
    }

    #[test]
    fn parse_hex_pattern_reports_non_ascii_character() {
        // A multibyte char must be reported verbatim, not truncated to a
        // wrong Latin-1 byte. "é" is two UTF-8 bytes; "é0" is two *chars*.
        match parse_hex_pattern("é0") {
            Err(HexParseError::InvalidCharacter('é')) => {}
            other => panic!("expected InvalidCharacter('é'), got {other:?}"),
        }
        // A lone multibyte char is an odd char count -> InvalidLength, not a
        // spurious even byte-length pass.
        match parse_hex_pattern("é") {
            Err(HexParseError::InvalidLength) => {}
            other => panic!("expected InvalidLength, got {other:?}"),
        }
    }

    #[test]
    fn first_difference_returns_first_offset() {
        let result = compare_binary(
            "left",
            b"\x00\x01\x02\x03",
            "right",
            b"\x00\xFF\x02\xFF",
            &BinaryCompareOptions::default(),
        );
        assert_eq!(result.first_difference(), Some(1));
    }

    #[test]
    fn first_difference_returns_none_when_equal() {
        let result = compare_binary(
            "left",
            b"\x00\x01\x02",
            "right",
            b"\x00\x01\x02",
            &BinaryCompareOptions::default(),
        );
        assert_eq!(result.first_difference(), None);
    }

    #[test]
    fn next_difference_after_skips_current() {
        let result = compare_binary(
            "left",
            b"\x00\xFF\x00\xFF\x00",
            "right",
            b"\x00\x00\x00\x00\x00",
            &BinaryCompareOptions::default(),
        );
        assert_eq!(result.next_difference_after(1), Some(3));
        assert_eq!(result.next_difference_after(3), None);
    }

    #[test]
    fn previous_difference_before_scans_backwards() {
        let result = compare_binary(
            "left",
            b"\xFF\x00\xFF\x00\xFF",
            "right",
            b"\x00\x00\x00\x00\x00",
            &BinaryCompareOptions::default(),
        );
        assert_eq!(result.previous_difference_before(4), Some(2));
        assert_eq!(result.previous_difference_before(2), Some(0));
        assert_eq!(result.previous_difference_before(0), None);
    }

    #[test]
    fn difference_at_returns_true_for_exact_offset() {
        let result = compare_binary(
            "left",
            b"\xFF\x00\xFF",
            "right",
            b"\x00\x00\x00",
            &BinaryCompareOptions::default(),
        );
        assert!(result.difference_at(0));
        assert!(!result.difference_at(1));
        assert!(result.difference_at(2));
    }

    #[test]
    fn hex_page_returns_correct_slice() {
        let result = compare_binary(
            "left",
            b"\x00\x01\x02\x03\x04\x05\x06\x07",
            "right",
            b"\x00\x01\x02\x03\x04\x05\x06\x07",
            &BinaryCompareOptions {
                bytes_per_row: 2,
                ..BinaryCompareOptions::default()
            },
        );
        let page = result.hex_page(1, 2);
        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, 2);
        assert_eq!(page.total_rows, 4);
        assert_eq!(page.start_offset, 4);
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0].offset, 4);
        assert_eq!(page.rows[1].offset, 6);
    }

    #[test]
    fn hex_page_out_of_range_returns_empty() {
        let result = compare_binary(
            "left",
            b"\x00\x01",
            "right",
            b"\x00\x01",
            &BinaryCompareOptions::default(),
        );
        let page = result.hex_page(99, 10);
        assert!(page.rows.is_empty());
        assert_eq!(page.total_rows, 1);
        assert_eq!(page.page, 99);
    }

    #[test]
    fn generate_hex_rows_produces_correct_count() {
        let rows = generate_hex_rows(b"\x00\x01\x02\x03\x04", b"\x00\x01\x02\x03\x04", 2);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].offset, 0);
        assert_eq!(rows[1].offset, 2);
        assert_eq!(rows[2].offset, 4);
        assert_eq!(rows[0].left_hex, "00 01");
        assert_eq!(rows[2].left_hex, "04");
        assert!(!rows[0].has_difference);
        assert!(!rows[1].has_difference);
        assert!(!rows[2].has_difference);
    }

    #[test]
    fn interpret_u16_le_reads_correctly() {
        let result = compare_binary(
            "left",
            b"\x01\x00\x02\x00",
            "right",
            b"\x01\x00\x02\x00",
            &BinaryCompareOptions::default(),
        );
        let interp = result.interpret_at(0, TypedValueKind::U16Le).unwrap();
        assert_eq!(interp.left_value, "1");
        assert_eq!(interp.right_value, "1");
        assert!(!interp.differs);

        let interp2 = result.interpret_at(2, TypedValueKind::U16Le).unwrap();
        assert_eq!(interp2.left_value, "2");
    }

    #[test]
    fn interpret_f32_be_detects_difference() {
        let left_bytes: Vec<u8> = 1.0f32.to_be_bytes().to_vec();
        let right_bytes: Vec<u8> = 2.0f32.to_be_bytes().to_vec();
        let result = compare_binary(
            "left",
            &left_bytes,
            "right",
            &right_bytes,
            &BinaryCompareOptions::default(),
        );
        let interp = result.interpret_at(0, TypedValueKind::F32Be).unwrap();
        assert_eq!(interp.left_value, "1");
        assert_eq!(interp.right_value, "2");
        assert!(interp.differs);
    }

    #[test]
    fn interpret_out_of_bounds_returns_none() {
        let result = compare_binary(
            "left",
            b"\x01\x02",
            "right",
            b"\x01\x02",
            &BinaryCompareOptions::default(),
        );
        assert!(result.interpret_at(1, TypedValueKind::U16Le).is_none());
        assert!(result.interpret_at(0, TypedValueKind::U32Le).is_none());
        assert!(result.interpret_at(100, TypedValueKind::U8).is_none());
    }

    #[test]
    fn compare_binary_files_rejects_oversize_content() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left.bin");
        let right = tmp.path().join("right.bin");
        let f = std::fs::File::create(&left).unwrap();
        f.set_len(MAX_BINARY_CONTENT_BYTES + 1).unwrap();
        drop(f);
        std::fs::write(&right, b"x").unwrap();
        let err =
            compare_binary_files(&left, &right, &BinaryCompareOptions::default()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
