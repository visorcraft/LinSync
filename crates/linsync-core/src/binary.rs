use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

impl BinaryCompareResult {
    pub fn is_equal(&self) -> bool {
        self.differences.is_empty() && self.metadata_differences.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryMetadataCompare {
    pub left: BinaryFileMetadata,
    pub right: BinaryFileMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryFileMetadata {
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub readonly: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteDiff {
    pub offset: usize,
    pub left: Option<u8>,
    pub right: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexRow {
    pub offset: usize,
    pub left_hex: String,
    pub right_hex: String,
    pub left_ascii: String,
    pub right_ascii: String,
    pub has_difference: bool,
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

        rows.push(HexRow {
            offset,
            left_hex: hex_bytes(left_chunk),
            right_hex: hex_bytes(right_chunk),
            left_ascii: ascii_preview(left_chunk),
            right_ascii: ascii_preview(right_chunk),
            has_difference: row_has_difference(offset, bytes_per_row, &differences),
        });
        offset += bytes_per_row;
    }

    (differences, rows)
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

fn row_has_difference(offset: usize, width: usize, differences: &[ByteDiff]) -> bool {
    let end = offset + width;
    differences
        .iter()
        .any(|difference| difference.offset >= offset && difference.offset < end)
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
}
