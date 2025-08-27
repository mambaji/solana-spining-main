use anyhow::Result;
use log::{debug, info};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::transaction::VersionedTransaction;
use std::str::FromStr;

pub const PUMP_PROGRAM_PUBKEY: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

#[derive(Debug, Clone)]
pub struct TransactionData {
    pub transaction: VersionedTransaction,
    pub slot: u64,
}

pub fn is_program_transaction(tx: &VersionedTransaction, program_id: &Pubkey) -> bool {
    let msg = &tx.message;
    let keys = msg.static_account_keys();
    
    for instr in msg.instructions() {
        let program_key = keys[instr.program_id_index as usize];
        if program_key == *program_id {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone)]
pub struct TokenCreationEvent {
    pub mint: Pubkey,
    pub slot: u64,
    pub signature: String,
}

pub struct TokenDetector {
    pump_program: Pubkey,
}

impl TokenDetector {
    pub fn new() -> Result<Self> {
        let pump_program = Pubkey::from_str(PUMP_PROGRAM_PUBKEY)?;
        Ok(Self { pump_program })
    }

    pub fn detect_token_creation(&self, tx_data: &TransactionData) -> Option<TokenCreationEvent> {
        if !is_program_transaction(&tx_data.transaction, &self.pump_program) {
            return None;
        }

        if let Some(mint) = self.extract_mint_from_create(&tx_data.transaction) {
            let signature = tx_data.transaction.signatures
                .get(0)
                .map(|s| s.to_string())
                .unwrap_or_default();

            debug!("Detected token creation: mint={}, slot={}", mint, tx_data.slot);

            return Some(TokenCreationEvent {
                mint,
                slot: tx_data.slot,
                signature,
            });
        }

        None
    }

    fn extract_mint_from_create(&self, tx: &VersionedTransaction) -> Option<Pubkey> {
        let msg = &tx.message;
        let keys = msg.static_account_keys();

        for instr in msg.instructions() {
            let program_key = keys[instr.program_id_index as usize];
            if program_key == self.pump_program {
                if instr.data.len() >= 8 {
                    let discriminator = &instr.data[..8];
                    if discriminator == [24, 30, 200, 40, 5, 28, 7, 119] {
                        if let Some(mint_key) = keys.get(instr.accounts[0] as usize) {
                            return Some(*mint_key);
                        }
                    }
                }
            }
        }

        None
    }
}

pub fn process_transaction_for_tokens(
    tx_data: TransactionData,
    detector: &TokenDetector,
) -> Result<()> {
    if let Some(token_event) = detector.detect_token_creation(&tx_data) {
        info!("🪙 发现新代币创建!");
        info!("   Mint: {}", token_event.mint);
        info!("   Slot: {}", token_event.slot);
        info!("   签名: {}", token_event.signature);
        info!("---");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_creation() {
        let detector = TokenDetector::new();
        assert!(detector.is_ok());
    }
}