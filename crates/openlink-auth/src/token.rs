//! Token生成与验证 - JWT双令牌机制
//!
//! access_token: 短期凭证(1小时)，用于API请求鉴权
//! refresh_token: 长期凭证(1天)，用于刷新access_token

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use super::session::Session;
use crate::error::TokenError;

type HmacSha256 = Hmac<Sha256>;

/// JWT Header
#[derive(Debug, Serialize, Deserialize)]
struct JwtHeader {
    alg: String,
    typ: String,
}

/// Access Token Claims
#[derive(Debug, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    /// session_id
    pub sub: String,
    /// permission_id
    pub permission_id: String,
    /// agent_id
    pub agent_id: String,
    /// user_id
    pub user_id: String,
    /// 过期时间(Unix timestamp)
    pub exp: i64,
    /// 签发时间
    pub iat: i64,
    /// JWT ID（用于吊销）
    pub jti: String,
}

/// Refresh Token Claims
#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshTokenClaims {
    /// session_id
    pub sub: String,
    /// 过期时间
    pub exp: i64,
    /// 签发时间
    pub iat: i64,
    /// JWT ID（一次性使用）
    pub jti: String,
}

/// Token生成器
#[derive(Debug, Clone)]
pub struct TokenGenerator {
    /// 签名密钥
    secret_key: Vec<u8>,
}

impl TokenGenerator {
    /// 创建Token生成器
    pub fn new(secret_key: &[u8]) -> Self {
        Self {
            secret_key: secret_key.to_vec(),
        }
    }

    /// 从配置字符串创建
    pub fn from_string(secret: &str) -> Self {
        Self::new(secret.as_bytes())
    }

    /// 生成Access Token
    pub fn generate_access_token(&self, session: &Session) -> Result<String, TokenError> {
        let claims = AccessTokenClaims {
            sub: session.session_id.clone(),
            permission_id: session.permission_id.clone(),
            agent_id: session.agent_id.clone(),
            user_id: session.user_id.clone(),
            exp: session.expires_at.timestamp(),
            iat: session.issued_at.timestamp(),
            jti: Uuid::new_v4().to_string(),
        };

        self.encode_token(&claims)
    }

    /// 生成Refresh Token
    pub fn generate_refresh_token(&self, session: &Session) -> Result<String, TokenError> {
        let claims = RefreshTokenClaims {
            sub: session.session_id.clone(),
            exp: session.expires_at.timestamp() + 86400, // 额外1天
            iat: chrono::Utc::now().timestamp(),
            jti: Uuid::new_v4().to_string(),
        };

        self.encode_token(&claims)
    }

    /// 验证Access Token
    pub fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims, TokenError> {
        let (header_b64, payload_b64, sig_b64) = self.split_token(token)?;
        self.verify_signature(&format!("{}.{}", header_b64, payload_b64), &sig_b64)?;

        let payload = Self::base64_decode(&payload_b64).ok_or(TokenError::Decode("Invalid payload encoding".into()))?;

        let claims: AccessTokenClaims =
            serde_json::from_slice(&payload).map_err(|e| TokenError::Decode(e.to_string()))?;

        // 检查过期
        let now = chrono::Utc::now().timestamp();
        if claims.exp < now {
            return Err(TokenError::Expired);
        }

        Ok(claims)
    }

    /// 验证Refresh Token
    pub fn verify_refresh_token(&self, token: &str) -> Result<RefreshTokenClaims, TokenError> {
        let (header_b64, payload_b64, sig_b64) = self.split_token(token)?;
        self.verify_signature(&format!("{}.{}", header_b64, payload_b64), &sig_b64)?;

        let payload = Self::base64_decode(&payload_b64).ok_or(TokenError::Decode("Invalid payload encoding".into()))?;

        let claims: RefreshTokenClaims =
            serde_json::from_slice(&payload).map_err(|e| TokenError::Decode(e.to_string()))?;

        let now = chrono::Utc::now().timestamp();
        if claims.exp < now {
            return Err(TokenError::Expired);
        }

        Ok(claims)
    }

    /// 编码token（简化JWT实现）
    fn encode_token<T: Serialize>(&self, claims: &T) -> Result<String, TokenError> {
        let header = JwtHeader {
            alg: "HS256".into(),
            typ: "JWT".into(),
        };

        let header_json = serde_json::to_vec(&header).map_err(|e| TokenError::Encode(e.to_string()))?;
        let payload_json = serde_json::to_vec(claims).map_err(|e| TokenError::Encode(e.to_string()))?;

        let header_b64 = Self::base64_encode(&header_json);
        let payload_b64 = Self::base64_encode(&payload_json);

        let signing_input = format!("{}.{}", header_b64, payload_b64);
        let signature = self.compute_signature(&signing_input);

        Ok(format!("{}.{}", signing_input, signature))
    }

    /// 计算HMAC-SHA256签名
    fn compute_signature(&self, input: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.secret_key).expect("HMAC key length is valid");
        mac.update(input.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    /// 验证签名
    fn verify_signature(&self, signing_input: &str, expected_sig: &str) -> Result<(), TokenError> {
        let actual_sig = self.compute_signature(signing_input);
        if actual_sig == expected_sig {
            Ok(())
        } else {
            Err(TokenError::InvalidSignature)
        }
    }

    /// 分割token为三部分
    fn split_token(&self, token: &str) -> Result<(String, String, String), TokenError> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(TokenError::Decode("Invalid token format".into()));
        }
        Ok((parts[0].into(), parts[1].into(), parts[2].into()))
    }

    /// Base64编码
    fn base64_encode(data: &[u8]) -> String {
        base64_simd::URL_SAFE_NO_PAD.encode_to_string(data)
    }

    /// Base64解码
    fn base64_decode(encoded: &str) -> Option<Vec<u8>> {
        base64_simd::URL_SAFE_NO_PAD.decode_to_vec(encoded).ok()
    }
}

// 简化的base64实现（避免额外依赖）
mod base64_simd {
    pub struct UrlSafeNoPad;

    impl UrlSafeNoPad {
        pub fn encode_to_string(&self, data: &[u8]) -> String {
            const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut result = String::new();
            let chunks = data.chunks(3);

            for chunk in chunks {
                let b0 = chunk[0] as u32;
                let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
                let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };

                let triple = (b0 << 16) | (b1 << 8) | b2;

                result.push(CHARSET[((triple >> 18) & 0x3F) as usize] as char);
                result.push(CHARSET[((triple >> 12) & 0x3F) as usize] as char);

                if chunk.len() > 1 {
                    result.push(CHARSET[((triple >> 6) & 0x3F) as usize] as char);
                }
                if chunk.len() > 2 {
                    result.push(CHARSET[(triple & 0x3F) as usize] as char);
                }
            }

            result
        }

        pub fn decode_to_vec(&self, encoded: &str) -> Result<Vec<u8>, ()> {
            const CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut result = Vec::new();
            let chars: Vec<char> = encoded.chars().collect();

            let chunks = chars.chunks(4);
            for chunk in chunks {
                if chunk.len() < 2 {
                    break;
                }

                let v0 = CHARSET.find(chunk[0]).ok_or(())? as u32;
                let v1 = CHARSET.find(chunk[1]).ok_or(())? as u32;
                let v2 = if chunk.len() > 2 {
                    CHARSET.find(chunk[2]).ok_or(())? as u32
                } else {
                    0
                };
                let v3 = if chunk.len() > 3 {
                    CHARSET.find(chunk[3]).ok_or(())? as u32
                } else {
                    0
                };

                let triple = (v0 << 18) | (v1 << 12) | (v2 << 6) | v3;

                result.push(((triple >> 16) & 0xFF) as u8);
                if chunk.len() > 2 {
                    result.push(((triple >> 8) & 0xFF) as u8);
                }
                if chunk.len() > 3 {
                    result.push((triple & 0xFF) as u8);
                }
            }

            Ok(result)
        }
    }

    pub static URL_SAFE_NO_PAD: UrlSafeNoPad = UrlSafeNoPad;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionConfig;

    fn test_session() -> Session {
        Session::new(
            "perm-001".into(),
            "agent-001".into(),
            "user-001".into(),
            &SessionConfig::default(),
        )
    }

    #[test]
    fn test_generate_access_token() {
        let gen = TokenGenerator::from_string("test-secret-key");
        let session = test_session();
        let token = gen.generate_access_token(&session).unwrap();

        // 应该是 header.payload.signature 格式
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_verify_access_token() {
        let gen = TokenGenerator::from_string("test-secret-key");
        let session = test_session();
        let token = gen.generate_access_token(&session).unwrap();

        let claims = gen.verify_access_token(&token).unwrap();
        assert_eq!(claims.sub, session.session_id);
        assert_eq!(claims.agent_id, "agent-001");
        assert_eq!(claims.user_id, "user-001");
    }

    #[test]
    fn test_generate_and_verify_refresh_token() {
        let gen = TokenGenerator::from_string("test-secret-key");
        let session = test_session();
        let token = gen.generate_refresh_token(&session).unwrap();

        let claims = gen.verify_refresh_token(&token).unwrap();
        assert_eq!(claims.sub, session.session_id);
    }

    #[test]
    fn test_invalid_signature() {
        let gen1 = TokenGenerator::from_string("secret-1");
        let gen2 = TokenGenerator::from_string("secret-2");
        let session = test_session();

        let token = gen1.generate_access_token(&session).unwrap();
        let result = gen2.verify_access_token(&token);
        assert!(result.is_err());
    }

    #[test]
    fn test_malformed_token() {
        let gen = TokenGenerator::from_string("test-secret");
        let result = gen.verify_access_token("not.a.valid-token");
        // 可能签名验证失败或解码失败
        assert!(result.is_err());
    }
}
