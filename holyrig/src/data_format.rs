use std::fmt::Display;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataFormatError {
    #[error("Invalid format name: {0}")]
    InvalidName(String),
    #[error("Number {value} is too long to fit in {length} bytes")]
    NumberTooLong { value: i32, length: usize },
    #[error("Negative number {value} is not supported by format {format}")]
    NegativeNotSupported { value: i32, format: DataFormat },
    #[error("Invalid BCD digit {byte:#X} at position {position}")]
    InvalidBcdDigit { byte: u8, position: usize },
    #[error("Input data is empty")]
    EmptyInput,
    #[error("Invalid text format byte {byte:#X} at position {position}")]
    InvalidTextFormat { byte: u8, position: usize },
    #[error("Invalid hex digit {byte:#X} at position {position}")]
    InvalidHexDigit { byte: u8, position: usize },
    #[error("Number {value} is out of i32 range")]
    NumberOutOfRange { value: i64 },
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DataFormat {
    BcdBs,
    BcdBu,
    BcdLs,
    BcdLu,
    IntBs,
    IntBu,
    IntLs,
    IntLu,
    Text,
    Hex,
}

impl Display for DataFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self {
            DataFormat::BcdBs => "bcd_bs",
            DataFormat::BcdBu => "bcd_bu",
            DataFormat::BcdLs => "bcd_ls",
            DataFormat::BcdLu => "bcd_lu",
            DataFormat::IntBs => "int_bs",
            DataFormat::IntBu => "int_bu",
            DataFormat::IntLs => "int_ls",
            DataFormat::IntLu => "int_lu",
            DataFormat::Text => "text",
            DataFormat::Hex => "hex",
        };
        write!(f, "{result}")
    }
}

impl TryFrom<&str> for DataFormat {
    type Error = DataFormatError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let result = match value {
            "bcd_bs" => DataFormat::BcdBs,
            "bcd_bu" => DataFormat::BcdBu,
            "bcd_ls" => DataFormat::BcdLs,
            "bcd_lu" => DataFormat::BcdLu,
            "int_bs" => DataFormat::IntBs,
            "int_bu" => DataFormat::IntBu,
            "int_ls" => DataFormat::IntLs,
            "int_lu" => DataFormat::IntLu,
            "text" => DataFormat::Text,
            "hex" => DataFormat::Hex,
            _ => {
                return Err(DataFormatError::InvalidName(value.to_string()));
            }
        };
        Ok(result)
    }
}

impl DataFormat {
    // Add these helper functions before the encode method
    fn get_significant_bytes_signed(value: i32) -> usize {
        let bits_needed = if value < 0 {
            32 - value.leading_ones()
        } else {
            32 - value.leading_zeros()
        };
        bits_needed.div_ceil(8) as usize
    }

    fn get_significant_bytes_unsigned(value: u32) -> usize {
        (32 - value.leading_zeros()).div_ceil(8) as usize
    }

    fn encode_bcd_bs(value: i32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        let mut result = vec![0; length];
        let abs_value = value.abs();
        let mut digits = abs_value.to_string();

        // TODO: convert to digits without formatting strings
        if !digits.len().is_multiple_of(2) {
            digits = format!("0{digits}");
        }

        let mut bcd_bytes = Vec::new();
        for chunk in digits.as_bytes().chunks(2) {
            let high = (chunk[0] - b'0') << 4;
            let low = chunk[1] - b'0';
            bcd_bytes.push(high | low);
        }

        if bcd_bytes.len() > length {
            return Err(DataFormatError::NumberTooLong { value, length });
        }

        let start = result.len() - bcd_bytes.len();
        result[start..].copy_from_slice(&bcd_bytes);

        if value < 0 {
            result[0] = 0xFF;
        }

        Ok(result)
    }

    fn encode_bcd_bu(value: i32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        if value < 0 {
            return Err(DataFormatError::NegativeNotSupported {
                value,
                format: DataFormat::BcdBu,
            });
        }
        Self::encode_bcd_bs(value, length)
    }

    fn encode_bcd_ls(value: i32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        let mut result = Self::encode_bcd_bs(value, length)?;
        result.reverse();
        Ok(result)
    }

    fn encode_bcd_lu(value: i32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        if value < 0 {
            return Err(DataFormatError::NegativeNotSupported {
                value,
                format: DataFormat::BcdLu,
            });
        }
        Self::encode_bcd_ls(value, length)
    }

    fn encode_int_bs(value: i32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        let mut result = vec![0; length];
        let bytes = value.to_be_bytes();
        let significant_bytes = Self::get_significant_bytes_signed(value);

        if significant_bytes > length {
            return Err(DataFormatError::NumberTooLong { value, length });
        }

        let start = result.len() - significant_bytes;
        result[start..].copy_from_slice(&bytes[bytes.len() - significant_bytes..]);

        if value < 0 {
            result[..start].fill(0xFF);
        }

        Ok(result)
    }

    fn encode_int_bu(value: u32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        let mut result = vec![0; length];
        let bytes = value.to_be_bytes();
        let significant_bytes = Self::get_significant_bytes_unsigned(value);

        if significant_bytes > length {
            return Err(DataFormatError::NumberTooLong {
                value: value as i32,
                length,
            });
        }

        let start = result.len() - significant_bytes;
        result[start..].copy_from_slice(&bytes[bytes.len() - significant_bytes..]);
        Ok(result)
    }

    fn encode_int_ls(value: i32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        let mut result = vec![0; length];
        let bytes = value.to_le_bytes();
        let significant_bytes = Self::get_significant_bytes_signed(value);

        if significant_bytes > length {
            return Err(DataFormatError::NumberTooLong { value, length });
        }

        result[..significant_bytes].copy_from_slice(&bytes[..significant_bytes]);

        if value < 0 {
            result[significant_bytes..].fill(0xFF);
        }

        Ok(result)
    }

    fn encode_int_lu(value: u32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        let mut result = vec![0; length];
        let bytes = value.to_le_bytes();
        let significant_bytes = Self::get_significant_bytes_unsigned(value);

        if significant_bytes > length {
            return Err(DataFormatError::NumberTooLong {
                value: value as i32,
                length,
            });
        }

        result[..significant_bytes].copy_from_slice(&bytes[..significant_bytes]);
        Ok(result)
    }

    fn encode_hex(value: i32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        if value < 0 {
            return Err(DataFormatError::NegativeNotSupported {
                value,
                format: DataFormat::Hex,
            });
        }
        let hex = format!("{value:X}");
        if hex.len() > length {
            return Err(DataFormatError::NumberTooLong { value, length });
        }
        let mut result = vec![b'0'; length];
        let start = length - hex.len();
        result[start..].copy_from_slice(hex.as_bytes());
        Ok(result)
    }

    fn encode_text(value: i32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        let text = value.to_string();
        if text.len() > length {
            return Err(DataFormatError::NumberTooLong { value, length });
        }
        let mut result = vec![b'0'; length];
        let start = length.saturating_sub(text.len());
        result[start..].copy_from_slice(text.as_bytes());
        Ok(result)
    }

    pub fn encode(&self, value: i32, length: usize) -> Result<Vec<u8>, DataFormatError> {
        match self {
            DataFormat::BcdBs => Self::encode_bcd_bs(value, length),
            DataFormat::BcdBu => Self::encode_bcd_bu(value, length),
            DataFormat::BcdLs => Self::encode_bcd_ls(value, length),
            DataFormat::BcdLu => Self::encode_bcd_lu(value, length),
            DataFormat::IntBs => Self::encode_int_bs(value, length),
            DataFormat::IntBu => Self::encode_int_bu(value as u32, length),
            DataFormat::IntLs => Self::encode_int_ls(value, length),
            DataFormat::IntLu => Self::encode_int_lu(value as u32, length),
            DataFormat::Text => Self::encode_text(value, length),
            DataFormat::Hex => Self::encode_hex(value, length),
        }
    }

    pub fn decode(&self, data: &[u8]) -> Result<i32, DataFormatError> {
        if data.is_empty() {
            return Err(DataFormatError::EmptyInput);
        }

        match self {
            DataFormat::BcdBs => Self::decode_bcd_bs(data),
            DataFormat::BcdBu => Self::decode_bcd_bu(data),
            DataFormat::BcdLs => Self::decode_bcd_ls(data),
            DataFormat::BcdLu => Self::decode_bcd_lu(data),
            DataFormat::IntBs => Self::decode_int_bs(data),
            DataFormat::IntBu => Self::decode_int_bu(data),
            DataFormat::IntLs => Self::decode_int_ls(data),
            DataFormat::IntLu => Self::decode_int_lu(data),
            DataFormat::Text => Self::decode_text(data),
            DataFormat::Hex => Self::decode_hex(data),
        }
    }

    fn decode_bcd_bs(data: &[u8]) -> Result<i32, DataFormatError> {
        let is_negative = data[0] == 0xFF;
        if is_negative && data.len() == 1 {
            return Err(DataFormatError::InvalidBcdDigit {
                byte: data[0],
                position: 0,
            });
        }

        let start = if is_negative { 1 } else { 0 };
        let mut skip_leading = true;
        let mut result = 0i64;

        for (i, &byte) in data[start..].iter().enumerate() {
            let high = (byte >> 4) & 0x0F;
            let low = byte & 0x0F;

            if high > 9 {
                return Err(DataFormatError::InvalidBcdDigit {
                    byte: high,
                    position: i * 2,
                });
            }
            if low > 9 {
                return Err(DataFormatError::InvalidBcdDigit {
                    byte: low,
                    position: i * 2 + 1,
                });
            }

            if skip_leading && high == 0 && low == 0 {
                continue;
            }

            // Process high digit if it's not a leading zero
            if high != 0 || !skip_leading {
                skip_leading = false;
                result = result * 10 + high as i64;
            }

            // Process low digit
            if low != 0 || !skip_leading {
                skip_leading = false;
                result = result * 10 + low as i64;
            }
        }

        if result > i32::MAX as i64 {
            return Err(DataFormatError::NumberOutOfRange { value: result });
        }

        Ok(if is_negative {
            -(result as i32)
        } else {
            result as i32
        })
    }

    fn decode_bcd_bu(data: &[u8]) -> Result<i32, DataFormatError> {
        let result = Self::decode_bcd_bs(data)?;
        if result < 0 {
            return Err(DataFormatError::NegativeNotSupported {
                value: result,
                format: DataFormat::BcdBu,
            });
        }
        Ok(result)
    }

    fn decode_bcd_ls(data: &[u8]) -> Result<i32, DataFormatError> {
        let mut reversed = data.to_vec();
        reversed.reverse();
        Self::decode_bcd_bs(&reversed)
    }

    fn decode_bcd_lu(data: &[u8]) -> Result<i32, DataFormatError> {
        let result = Self::decode_bcd_ls(data)?;
        if result < 0 {
            return Err(DataFormatError::NegativeNotSupported {
                value: result,
                format: DataFormat::BcdLu,
            });
        }
        Ok(result)
    }

    fn decode_int_bs(data: &[u8]) -> Result<i32, DataFormatError> {
        let mut bytes = [0u8; 4];
        let start = bytes.len() - data.len().min(4);
        let data_slice = &data[data.len().saturating_sub(4)..];
        bytes[start..].copy_from_slice(data_slice);

        // Fill with sign extension - check the first byte after alignment
        if bytes[start] & 0x80 != 0 {
            bytes[..start].fill(0xFF);
        }

        Ok(i32::from_be_bytes(bytes))
    }

    fn decode_int_bu(data: &[u8]) -> Result<i32, DataFormatError> {
        let mut bytes = [0u8; 4];
        let start = bytes.len() - data.len().min(4);
        bytes[start..].copy_from_slice(&data[data.len().saturating_sub(4)..]);

        let result = u32::from_be_bytes(bytes);
        if result > i32::MAX as u32 {
            return Err(DataFormatError::NumberOutOfRange {
                value: result as i64,
            });
        }
        Ok(result as i32)
    }

    fn decode_int_ls(data: &[u8]) -> Result<i32, DataFormatError> {
        let mut bytes = [0u8; 4];
        let len = data.len().min(4);
        bytes[..len].copy_from_slice(&data[..len]);

        if bytes[len - 1] & 0x80 != 0 {
            bytes[len..].fill(0xFF);
        }

        Ok(i32::from_le_bytes(bytes))
    }

    fn decode_int_lu(data: &[u8]) -> Result<i32, DataFormatError> {
        let mut bytes = [0u8; 4];
        bytes[..data.len().min(4)].copy_from_slice(&data[..data.len().min(4)]);

        let result = u32::from_le_bytes(bytes);
        if result > i32::MAX as u32 {
            return Err(DataFormatError::NumberOutOfRange {
                value: result as i64,
            });
        }
        Ok(result as i32)
    }

    fn decode_hex(data: &[u8]) -> Result<i32, DataFormatError> {
        let mut result = 0i64;
        for (i, &byte) in data.iter().enumerate() {
            let digit = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => return Err(DataFormatError::InvalidHexDigit { byte, position: i }),
            };
            result = result * 16 + digit as i64;
        }
        if result > i32::MAX as i64 {
            return Err(DataFormatError::NumberOutOfRange { value: result });
        }
        Ok(result as i32)
    }

    fn decode_text(data: &[u8]) -> Result<i32, DataFormatError> {
        let mut chars = Vec::with_capacity(data.len());
        let mut started = false;

        for (i, &byte) in data.iter().enumerate() {
            match byte {
                b'0' if !started => continue,
                b'0'..=b'9' => {
                    started = true;
                    chars.push(byte);
                }
                b'-' if !started => {
                    started = true;
                    chars.push(byte);
                }
                _ => return Err(DataFormatError::InvalidTextFormat { byte, position: i }),
            }
        }

        if chars.is_empty() {
            return Ok(0);
        }

        let text = String::from_utf8(chars).unwrap();
        text.parse().map_err(|_| DataFormatError::NumberOutOfRange {
            value: text.parse::<i64>().unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcd_formats() -> Result<(), DataFormatError> {
        let expected_bcd_b = vec![0x00, 0x00, 0x04, 0x18];
        assert_eq!(DataFormat::BcdBs.encode(418, 4)?, expected_bcd_b);
        assert_eq!(DataFormat::BcdBu.encode(418, 4)?, expected_bcd_b);

        let expected_bcd_l = vec![0x18, 0x04, 0x00, 0x00];
        assert_eq!(DataFormat::BcdLs.encode(418, 4)?, expected_bcd_l);
        assert_eq!(DataFormat::BcdLu.encode(418, 4)?, expected_bcd_l);

        let expected_bcd_bs_neg = vec![0xFF, 0x00, 0x04, 0x18];
        assert_eq!(DataFormat::BcdBs.encode(-418, 4)?, expected_bcd_bs_neg);

        let expected_bcd_ls_neg = vec![0x18, 0x04, 0x00, 0xFF];
        assert_eq!(DataFormat::BcdLs.encode(-418, 4)?, expected_bcd_ls_neg);
        Ok(())
    }

    #[test]
    fn test_int_formats() -> Result<(), DataFormatError> {
        let expected_int_b = vec![0x00, 0x00, 0x01, 0xA2];
        assert_eq!(DataFormat::IntBs.encode(418, 4)?, expected_int_b);
        assert_eq!(DataFormat::IntBu.encode(418, 4)?, expected_int_b);

        let expected_int_l = vec![0xA2, 0x01, 0x00, 0x00];
        assert_eq!(DataFormat::IntLs.encode(418, 4)?, expected_int_l);
        assert_eq!(DataFormat::IntLu.encode(418, 4)?, expected_int_l);

        let expected_int_bs_neg = vec![0xFF, 0xFF, 0xFE, 0x5E];
        assert_eq!(DataFormat::IntBs.encode(-418, 4)?, expected_int_bs_neg);

        let expected_int_ls_neg = vec![0x5E, 0xFE, 0xFF, 0xFF];
        assert_eq!(DataFormat::IntLs.encode(-418, 4)?, expected_int_ls_neg);
        Ok(())
    }

    #[test]
    fn test_text_format() -> Result<(), DataFormatError> {
        assert_eq!(DataFormat::Text.encode(5, 4)?, vec![0x30, 0x30, 0x30, 0x35]);
        assert_eq!(DataFormat::Text.encode(0, 4)?, vec![0x30, 0x30, 0x30, 0x30]);
        assert_eq!(DataFormat::Text.encode(7, 1)?, vec![0x37]);
        assert_eq!(
            DataFormat::Text.encode(42, 8)?,
            vec![0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x34, 0x32]
        );
        Ok(())
    }

    #[test]
    fn test_text_format_overflow() {
        assert!(matches!(
            DataFormat::Text.encode(12345, 4),
            Err(DataFormatError::NumberTooLong {
                value: 12345,
                length: 4
            })
        ));
    }

    #[test]
    fn test_text_format_negative_overflow() {
        assert!(matches!(
            DataFormat::Text.encode(-12345, 4),
            Err(DataFormatError::NumberTooLong {
                value: -12345,
                length: 4
            })
        ));
    }

    #[test]
    fn test_unsigned_negative() {
        assert!(matches!(
            DataFormat::BcdBu.encode(-418, 4),
            Err(DataFormatError::NegativeNotSupported {
                value: -418,
                format: DataFormat::BcdBu
            })
        ));
        assert!(matches!(
            DataFormat::BcdLu.encode(-418, 4),
            Err(DataFormatError::NegativeNotSupported {
                value: -418,
                format: DataFormat::BcdLu
            })
        ));
    }

    #[test]
    fn test_decode_bcd() -> Result<(), DataFormatError> {
        assert_eq!(DataFormat::BcdBs.decode(&[0x00, 0x00, 0x04, 0x18])?, 418);
        assert_eq!(DataFormat::BcdBu.decode(&[0x00, 0x00, 0x04, 0x18])?, 418);
        assert_eq!(DataFormat::BcdLs.decode(&[0x18, 0x04, 0x00, 0x00])?, 418);
        assert_eq!(DataFormat::BcdLu.decode(&[0x18, 0x04, 0x00, 0x00])?, 418);
        assert_eq!(DataFormat::BcdBs.decode(&[0xFF, 0x00, 0x04, 0x18])?, -418);
        assert_eq!(DataFormat::BcdLs.decode(&[0x18, 0x04, 0x00, 0xFF])?, -418);
        Ok(())
    }

    #[test]
    fn test_decode_int() -> Result<(), DataFormatError> {
        assert_eq!(DataFormat::IntBs.decode(&[0x00, 0x00, 0x01, 0xA2])?, 418);
        assert_eq!(DataFormat::IntBu.decode(&[0x00, 0x00, 0x01, 0xA2])?, 418);
        assert_eq!(DataFormat::IntLs.decode(&[0xA2, 0x01, 0x00, 0x00])?, 418);
        assert_eq!(DataFormat::IntLu.decode(&[0xA2, 0x01, 0x00, 0x00])?, 418);
        assert_eq!(DataFormat::IntBs.decode(&[0xFF, 0xFF, 0xFE, 0x5E])?, -418);
        assert_eq!(DataFormat::IntLs.decode(&[0x5E, 0xFE, 0xFF, 0xFF])?, -418);
        Ok(())
    }

    #[test]
    fn test_decode_text() -> Result<(), DataFormatError> {
        assert_eq!(DataFormat::Text.decode(b"0418")?, 418);
        assert_eq!(DataFormat::Text.decode(b"-418")?, -418);
        assert_eq!(DataFormat::Text.decode(b"0005")?, 5);
        assert_eq!(DataFormat::Text.decode(b"0000")?, 0);
        assert_eq!(DataFormat::Text.decode(b"7")?, 7);
        assert_eq!(DataFormat::Text.decode(b"00000042")?, 42);
        Ok(())
    }

    #[test]
    fn test_decode_invalid_bcd() {
        println!(
            "Ze: {:?}",
            DataFormat::BcdBs.decode(&[0x00, 0x00, 0x0A, 0x18])
        );
        assert!(matches!(
            DataFormat::BcdBs.decode(&[0x00, 0x00, 0x0A, 0x18]),
            Err(DataFormatError::InvalidBcdDigit {
                byte: 10,
                position: 5
            })
        ));
    }

    #[test]
    fn test_decode_invalid_text() {
        assert!(matches!(
            DataFormat::Text.decode(b"12A4"),
            Err(DataFormatError::InvalidTextFormat {
                byte: b'A',
                position: 2
            })
        ));
    }

    #[test]
    fn test_decode_empty() {
        assert!(matches!(
            DataFormat::BcdBs.decode(&[]),
            Err(DataFormatError::EmptyInput)
        ));
    }

    #[test]
    fn test_decode_unsigned_negative() {
        assert!(matches!(
            DataFormat::BcdBu.decode(&[0xFF, 0x00, 0x04, 0x18]),
            Err(DataFormatError::NegativeNotSupported {
                value: -418,
                format: DataFormat::BcdBu
            })
        ));
    }

    #[test]
    fn test_roundtrip() -> Result<(), DataFormatError> {
        let test_values = [-418, -1, 0, 1, 418];
        let formats = [
            DataFormat::BcdBs,
            DataFormat::IntBs,
            DataFormat::IntLs,
            DataFormat::Text,
        ];

        for &value in &test_values {
            for format in &formats {
                let encoded = format.encode(value, 4)?;
                let decoded = format.decode(&encoded)?;
                assert_eq!(
                    decoded, value,
                    "Round-trip failed for {value} with format {format}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn test_decode_bcd_edge_cases() -> Result<(), DataFormatError> {
        assert_eq!(DataFormat::BcdBs.decode(&[0x00, 0x00, 0x00, 0x05])?, 5);
        assert_eq!(DataFormat::BcdBs.decode(&[0x00, 0x00, 0x00, 0x42])?, 42);
        assert_eq!(DataFormat::BcdBs.decode(&[0x00, 0x00, 0x00, 0x00])?, 0);
        assert_eq!(
            DataFormat::BcdBs.decode(&[0x21, 0x47, 0x48, 0x36])?,
            21474836
        );
        assert_eq!(DataFormat::BcdBs.decode(&[0xFF, 0x00, 0x00, 0x00])?, 0);
        assert_eq!(DataFormat::BcdBs.decode(&[0x42])?, 42);
        Ok(())
    }

    #[test]
    fn test_decode_int_edge_cases() -> Result<(), DataFormatError> {
        assert_eq!(DataFormat::IntBs.decode(&[0x42])?, 66);
        assert_eq!(DataFormat::IntLs.decode(&[0x42])?, 66);
        assert_eq!(DataFormat::IntBs.decode(&[0xFF])?, -1);
        assert_eq!(DataFormat::IntLs.decode(&[0xFF])?, -1);
        assert_eq!(DataFormat::IntBs.decode(&[0xFF, 0xFE])?, -2);
        assert_eq!(DataFormat::IntLs.decode(&[0xFE, 0xFF])?, -2);
        assert_eq!(
            DataFormat::IntBs.decode(&[0x7F, 0xFF, 0xFF, 0xFF])?,
            i32::MAX
        );
        assert_eq!(
            DataFormat::IntLs.decode(&[0xFF, 0xFF, 0xFF, 0x7F])?,
            i32::MAX
        );
        assert_eq!(
            DataFormat::IntBs.decode(&[0x80, 0x00, 0x00, 0x00])?,
            i32::MIN
        );
        assert_eq!(
            DataFormat::IntLs.decode(&[0x00, 0x00, 0x00, 0x80])?,
            i32::MIN
        );

        Ok(())
    }

    #[test]
    fn test_decode_text_edge_cases() -> Result<(), DataFormatError> {
        assert!(matches!(
            DataFormat::Text.decode(b"-"),
            Err(DataFormatError::NumberOutOfRange { value: 0 })
        ));

        assert!(matches!(
            DataFormat::Text.decode(b"--123"),
            Err(DataFormatError::InvalidTextFormat {
                byte: b'-',
                position: 1
            })
        ));

        assert!(matches!(
            DataFormat::Text.decode(b"12-3"),
            Err(DataFormatError::InvalidTextFormat {
                byte: b'-',
                position: 2
            })
        ));

        assert_eq!(DataFormat::Text.decode(b"2147483647")?, i32::MAX);
        assert_eq!(DataFormat::Text.decode(b"-2147483648")?, i32::MIN);
        assert_eq!(DataFormat::Text.decode(b"-0042")?, -42);

        Ok(())
    }

    #[test]
    fn test_decode_bcd_invalid_cases() {
        assert!(matches!(
            DataFormat::BcdBs.decode(&[0x00, 0xA0, 0x00, 0x00]),
            Err(DataFormatError::InvalidBcdDigit {
                byte: 10,
                position: 2
            })
        ));

        assert!(matches!(
            DataFormat::BcdBs.decode(&[0x00, 0x0A, 0x00, 0x00]),
            Err(DataFormatError::InvalidBcdDigit {
                byte: 10,
                position: 3
            })
        ));

        assert!(matches!(
            DataFormat::BcdBs.decode(&[0x22, 0x47, 0x48, 0x36, 0x48]),
            Err(DataFormatError::NumberOutOfRange { value: 2247483648 })
        ));
    }

    #[test]
    fn test_decode_int_invalid_cases() {
        assert!(matches!(
            DataFormat::IntBu.decode(&[0xFF, 0xFF, 0xFF, 0xFF]),
            Err(DataFormatError::NumberOutOfRange { value: 4294967295 })
        ));
        assert!(matches!(
            DataFormat::IntLu.decode(&[0xFF, 0xFF, 0xFF, 0xFF]),
            Err(DataFormatError::NumberOutOfRange { value: 4294967295 })
        ));
    }

    #[test]
    fn test_encode_int_variable_length() -> Result<(), DataFormatError> {
        assert_eq!(DataFormat::IntBs.encode(0x12, 1)?, vec![0x12]);
        assert_eq!(DataFormat::IntBs.encode(-0x12, 1)?, vec![0xEE]);
        assert_eq!(DataFormat::IntBs.encode(0x1234, 2)?, vec![0x12, 0x34]);
        assert_eq!(DataFormat::IntBs.encode(-0x1234, 2)?, vec![0xED, 0xCC]);
        assert_eq!(DataFormat::IntBs.encode(0x12, 2)?, vec![0x00, 0x12]);
        assert_eq!(DataFormat::IntBs.encode(-0x12, 2)?, vec![0xFF, 0xEE]);
        assert_eq!(
            DataFormat::IntBs.encode(0x123456, 4)?,
            vec![0x00, 0x12, 0x34, 0x56]
        );
        assert_eq!(
            DataFormat::IntBs.encode(-0x123456, 4)?,
            vec![0xFF, 0xED, 0xCB, 0xAA]
        );

        assert_eq!(DataFormat::IntBu.encode(0x12, 1)?, vec![0x12]);
        assert_eq!(DataFormat::IntBu.encode(0xFF, 1)?, vec![0xFF]);
        assert_eq!(DataFormat::IntBu.encode(0x1234, 2)?, vec![0x12, 0x34]);
        assert_eq!(DataFormat::IntBu.encode(0x12, 2)?, vec![0x00, 0x12]);
        assert_eq!(
            DataFormat::IntBu.encode(0x123456, 3)?,
            vec![0x12, 0x34, 0x56]
        );

        assert_eq!(DataFormat::IntLs.encode(0x12, 1)?, vec![0x12]);
        assert_eq!(DataFormat::IntLs.encode(-0x12, 1)?, vec![0xEE]);
        assert_eq!(DataFormat::IntLs.encode(0x1234, 2)?, vec![0x34, 0x12]);
        assert_eq!(DataFormat::IntLs.encode(-0x1234, 2)?, vec![0xCC, 0xED]);
        assert_eq!(DataFormat::IntLs.encode(0x12, 2)?, vec![0x12, 0x00]);
        assert_eq!(DataFormat::IntLs.encode(-0x12, 2)?, vec![0xEE, 0xFF]);
        assert_eq!(
            DataFormat::IntLs.encode(0x123456, 4)?,
            vec![0x56, 0x34, 0x12, 0x00]
        );
        assert_eq!(
            DataFormat::IntLs.encode(-0x123456, 4)?,
            vec![0xAA, 0xCB, 0xED, 0xFF]
        );

        assert_eq!(DataFormat::IntLu.encode(0x12, 1)?, vec![0x12]);
        assert_eq!(DataFormat::IntLu.encode(0xFF, 1)?, vec![0xFF]);
        assert_eq!(DataFormat::IntLu.encode(0x1234, 2)?, vec![0x34, 0x12]);
        assert_eq!(DataFormat::IntLu.encode(0x12, 2)?, vec![0x12, 0x00]);
        assert_eq!(
            DataFormat::IntLu.encode(0x123456, 3)?,
            vec![0x56, 0x34, 0x12]
        );

        for format in [
            DataFormat::IntBs,
            DataFormat::IntBu,
            DataFormat::IntLs,
            DataFormat::IntLu,
        ] {
            assert!(matches!(
                format.encode(0x100, 1),
                Err(DataFormatError::NumberTooLong {
                    value: 256,
                    length: 1
                })
            ));

            assert!(matches!(
                format.encode(0x10000, 2),
                Err(DataFormatError::NumberTooLong {
                    value: 65536,
                    length: 2
                })
            ));
        }

        Ok(())
    }

    #[test]
    fn test_hex_format() -> Result<(), DataFormatError> {
        // Encode: number -> ASCII hex, zero-padded
        assert_eq!(DataFormat::Hex.encode(418, 4)?, b"01A2".to_vec());
        assert_eq!(DataFormat::Hex.encode(255, 2)?, b"FF".to_vec());
        assert_eq!(DataFormat::Hex.encode(0, 4)?, b"0000".to_vec());
        assert_eq!(DataFormat::Hex.encode(15, 1)?, b"F".to_vec());
        assert_eq!(DataFormat::Hex.encode(16, 2)?, b"10".to_vec());

        // Decode: ASCII hex -> number
        assert_eq!(DataFormat::Hex.decode(b"01A2")?, 418);
        assert_eq!(DataFormat::Hex.decode(b"FF")?, 255);
        assert_eq!(DataFormat::Hex.decode(b"0000")?, 0);
        assert_eq!(DataFormat::Hex.decode(b"ff")?, 255);
        assert_eq!(DataFormat::Hex.decode(b"F")?, 15);
        assert_eq!(DataFormat::Hex.decode(b"10")?, 16);

        // Negative not supported
        assert!(matches!(
            DataFormat::Hex.encode(-1, 4),
            Err(DataFormatError::NegativeNotSupported {
                value: -1,
                format: DataFormat::Hex
            })
        ));

        // Overflow
        assert!(matches!(
            DataFormat::Hex.encode(256, 1),
            Err(DataFormatError::NumberTooLong {
                value: 256,
                length: 1
            })
        ));

        // Invalid hex digit
        assert!(matches!(
            DataFormat::Hex.decode(b"GG"),
            Err(DataFormatError::InvalidHexDigit {
                byte: b'G',
                position: 0
            })
        ));

        Ok(())
    }

    #[test]
    fn test_decode_variable_length() -> Result<(), DataFormatError> {
        assert_eq!(
            DataFormat::IntBs.decode(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0x5E])?,
            -418
        );
        assert_eq!(
            DataFormat::IntLs.decode(&[0x5E, 0xFE, 0xFF, 0xFF, 0xFF, 0xFF])?,
            -418
        );

        assert_eq!(
            DataFormat::IntBu.decode(&[0x00, 0x00, 0x00, 0x00, 0x01, 0xA2])?,
            418
        );
        assert_eq!(
            DataFormat::IntLu.decode(&[0xA2, 0x01, 0x00, 0x00, 0x00, 0x00])?,
            418
        );

        Ok(())
    }

    #[test]
    fn test_decode_short_length() -> Result<(), DataFormatError> {
        assert_eq!(DataFormat::IntBs.decode(&[0xFF, 0xFE])?, -2);
        assert_eq!(DataFormat::IntLs.decode(&[0xFE, 0xFF])?, -2);
        Ok(())
    }
}
