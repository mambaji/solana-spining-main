use thiserror::Error;
use solana_sdk::signature::SignerError;
use solana_client::client_error::ClientError;

/// 交易执行错误类型
#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("Transaction failed: {message}, signature: {signature:?}")]
    TransactionFailed { 
        message: String, 
        signature: Option<String> 
    },

    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { 
        required: u64, 
        available: u64 
    },

    #[error("Slippage exceeded: expected {expected}, actual {actual}")]
    SlippageExceeded { 
        expected: u64, 
        actual: u64 
    },

    #[error("Service unavailable: {service} - {reason}")]
    ServiceUnavailable { 
        service: String, 
        reason: String 
    },

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Timeout occurred: operation took longer than {timeout_ms}ms")]
    Timeout { 
        timeout_ms: u64 
    },

    #[error("Authentication failed for service: {service}")]
    AuthenticationFailed { 
        service: String 
    },

    #[error("Rate limit exceeded for service: {service}, retry after {retry_after_seconds}s")]
    RateLimitExceeded { 
        service: String, 
        retry_after_seconds: u64 
    },

    #[error("Transaction serialization error: {0}")]
    Serialization(String),

    #[error("Signature error: {0}")]
    Signature(String),

    #[error("Blockhash not found or expired")]
    BlockhashExpired,

    #[error("Bundle creation failed: {reason}")]
    BundleCreationFailed { 
        reason: String 
    },

    #[error("Multiple execution strategies failed")]
    AllStrategiesFailed {
        attempts: Vec<(String, String)>, // (strategy_name, error_message)
    },

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<ClientError> for ExecutionError {
    fn from(err: ClientError) -> Self {
        ExecutionError::Network(err.to_string())
    }
}

impl From<SignerError> for ExecutionError {
    fn from(err: SignerError) -> Self {
        ExecutionError::Signature(err.to_string())
    }
}

impl From<std::io::Error> for ExecutionError {
    fn from(err: std::io::Error) -> Self {
        ExecutionError::Network(format!("IO Error: {}", err))
    }
}

impl From<serde_json::Error> for ExecutionError {
    fn from(err: serde_json::Error) -> Self {
        ExecutionError::Serialization(err.to_string())
    }
}

impl From<reqwest::Error> for ExecutionError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            ExecutionError::Timeout { timeout_ms: 30000 }
        } else if err.is_connect() {
            ExecutionError::Network(format!("Connection error: {}", err))
        } else {
            ExecutionError::Network(err.to_string())
        }
    }
}

impl ExecutionError {
    /// 检查错误是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ExecutionError::Network(_) |
            ExecutionError::Timeout { .. } |
            ExecutionError::ServiceUnavailable { .. } |
            ExecutionError::RateLimitExceeded { .. } |
            ExecutionError::BlockhashExpired
        )
    }

    /// 获取重试延迟建议 (毫秒)
    pub fn retry_delay_ms(&self) -> u64 {
        match self {
            ExecutionError::RateLimitExceeded { retry_after_seconds, .. } => {
                retry_after_seconds * 1000
            }
            ExecutionError::Network(_) => 1000,
            ExecutionError::Timeout { .. } => 2000,
            ExecutionError::ServiceUnavailable { .. } => 5000,
            ExecutionError::BlockhashExpired => 500,
            _ => 1000,
        }
    }
}