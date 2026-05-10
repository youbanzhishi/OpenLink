//! # 短码生成器
//!
//! 基于 Base62 的短码生成，6 位短码提供 568 亿组合空间。
//! Base62 字符集：0-9 A-Z a-z，保证 URL 安全，无歧义字符。

use rand::Rng;

/// Base62 字符集：0-9 + A-Z + a-z
const BASE62_CHARSET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// 默认短码长度（6 位 = 568 亿组合）
pub const DEFAULT_CODE_LENGTH: usize = 6;

/// 生成随机 Base62 短码
///
/// # Arguments
/// * `length` - 短码长度，默认 6
///
/// # Returns
/// Base62 编码的短码字符串
pub fn generate(length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..62);
            BASE62_CHARSET[idx] as char
        })
        .collect()
}

/// 生成默认长度（6位）的短码
pub fn generate_default() -> String {
    generate(DEFAULT_CODE_LENGTH)
}

/// 校验短码是否合法（仅含 Base62 字符）
pub fn is_valid(code: &str) -> bool {
    !code.is_empty() && code.chars().all(|c| BASE62_CHARSET.contains(&(c as u8)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_default_length() {
        let code = generate_default();
        assert_eq!(code.len(), DEFAULT_CODE_LENGTH);
    }

    #[test]
    fn test_generate_custom_length() {
        let code = generate(8);
        assert_eq!(code.len(), 8);
    }

    #[test]
    fn test_generate_is_valid() {
        for _ in 0..100 {
            let code = generate_default();
            assert!(is_valid(&code), "Generated code should be valid: {}", code);
        }
    }

    #[test]
    fn test_is_valid_rejects_invalid() {
        assert!(!is_valid(""));
        assert!(!is_valid("abc-def")); // 含短横线
        assert!(!is_valid("abc def")); // 含空格
        assert!(!is_valid("abc/123")); // 含斜杠
    }

    #[test]
    fn test_is_valid_accepts_base62() {
        assert!(is_valid("abc123"));
        assert!(is_valid("ABC123"));
        assert!(is_valid("0Zz9"));
    }
}
