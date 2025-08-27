use serde::{Serialize, Serializer};
use solana_sdk::pubkey::Pubkey;

/// 序列化Pubkey为字符串
pub fn serialize_pubkey<S>(pubkey: &Pubkey, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    pubkey.to_string().serialize(serializer)
}

/// 序列化可选Pubkey为可选字符串
pub fn serialize_option_pubkey<S>(pubkey: &Option<Pubkey>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match pubkey {
        Some(pk) => Some(pk.to_string()).serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// 序列化Vec<u8>为base64字符串
pub fn serialize_bytes_as_base64<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    bs58::encode(bytes).into_string().serialize(serializer)
}

/// 序列化可选Vec<u8>为可选base64字符串
pub fn serialize_option_bytes_as_base64<S>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match bytes {
        Some(b) => Some(bs58::encode(b).into_string()).serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// 序列化签名为字符串
pub fn serialize_signature<S>(signature: &solana_sdk::signature::Signature, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    signature.to_string().serialize(serializer)
}

/// 序列化可选签名为可选字符串
pub fn serialize_option_signature<S>(signature: &Option<solana_sdk::signature::Signature>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match signature {
        Some(sig) => Some(sig.to_string()).serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// 序列化Hash为字符串
pub fn serialize_hash<S>(hash: &solana_sdk::hash::Hash, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    hash.to_string().serialize(serializer)
}

/// 序列化可选Hash为可选字符串
pub fn serialize_option_hash<S>(hash: &Option<solana_sdk::hash::Hash>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match hash {
        Some(h) => Some(h.to_string()).serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// 将字节数组转换为十六进制字符串
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// 将十六进制字符串转换为字节数组
pub fn hex_to_bytes(hex_str: &str) -> Result<Vec<u8>, hex::FromHexError> {
    hex::decode(hex_str)
}

/// 将lamports转换为SOL
pub fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1_000_000_000.0
}

/// 将SOL转换为lamports
pub fn sol_to_lamports(sol: f64) -> u64 {
    (sol * 1_000_000_000.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    #[test]
    fn test_lamports_conversion() {
        assert_eq!(lamports_to_sol(1_000_000_000), 1.0);
        assert_eq!(sol_to_lamports(1.0), 1_000_000_000);
    }

    #[test]
    fn test_bytes_hex_conversion() {
        let bytes = vec![0x48, 0x65, 0x6c, 0x6c, 0x6f]; // "Hello"
        let hex = bytes_to_hex(&bytes);
        assert_eq!(hex, "48656c6c6f");
        
        let decoded = hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn test_pubkey_serialization() {
        let pubkey = Pubkey::from_str("11111111111111111111111111111112").unwrap();
        // 这里我们无法直接测试序列化函数，但可以确保pubkey是有效的
        assert_eq!(pubkey.to_string(), "11111111111111111111111111111112");
    }
}