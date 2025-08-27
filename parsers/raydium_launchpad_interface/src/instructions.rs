use crate::*;
use borsh::{BorshDeserialize, BorshSerialize};

use typedefs::{MintParams, CurveParams, VestingParams};
use std::io::Read;
use strum_macros::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, EnumString, Display)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RaydiumLaunchpadProgramIx {
    BuyExactIn(BuyExactInIxArgs),
    BuyExactOut(BuyExactOutIxArgs),
    Initialize(InitializeIxArgs),
    SellExactIn(SellExactInIxArgs),
    SellExactOut(SellExactOutIxArgs),
}

impl RaydiumLaunchpadProgramIx {
    pub fn name(&self) -> &str {
        match self {
            Self::BuyExactIn(_) => "BuyExactIn",
            Self::BuyExactOut(_) => "BuyExactOut", 
            Self::Initialize(_) => "Initialize",
            Self::SellExactIn(_) => "SellExactIn",
            Self::SellExactOut(_) => "SellExactOut",
        }
    }

    pub fn deserialize(buf: &[u8]) -> std::io::Result<Self> {
        let mut reader = buf;
        let mut maybe_discm = [0u8; 8];
        reader.read_exact(&mut maybe_discm)?;
        match maybe_discm {
            BUY_EXACT_IN_IX_DISCM => {
                Ok(Self::BuyExactIn(BuyExactInIxArgs::deserialize(&mut reader)?))
            }
            BUY_EXACT_OUT_IX_DISCM => {
                Ok(Self::BuyExactOut(BuyExactOutIxArgs::deserialize(&mut reader)?))
            }
            INITIALIZE_IX_DISCM => {
                Ok(Self::Initialize(InitializeIxArgs::deserialize(&mut reader)?))
            }
            SELL_EXACT_IN_IX_DISCM => {
                Ok(Self::SellExactIn(SellExactInIxArgs::deserialize(&mut reader)?))
            }
            SELL_EXACT_OUT_IX_DISCM => {
                Ok(Self::SellExactOut(SellExactOutIxArgs::deserialize(&mut reader)?))
            }
            _ => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("discm {:?} not found", maybe_discm),
                ))
            }
        }
    }

    pub fn serialize<W: std::io::Write>(&self, mut writer: W) -> std::io::Result<()> {
        match self {
            Self::BuyExactIn(args) => {
                writer.write_all(&BUY_EXACT_IN_IX_DISCM)?;
                args.serialize(&mut writer)
            }
            Self::BuyExactOut(args) => {
                writer.write_all(&BUY_EXACT_OUT_IX_DISCM)?;
                args.serialize(&mut writer)
            }
            Self::Initialize(args) => {
                writer.write_all(&INITIALIZE_IX_DISCM)?;
                args.serialize(&mut writer)
            }
            Self::SellExactIn(args) => {
                writer.write_all(&SELL_EXACT_IN_IX_DISCM)?;
                args.serialize(&mut writer)
            }
            Self::SellExactOut(args) => {
                writer.write_all(&SELL_EXACT_OUT_IX_DISCM)?;
                args.serialize(&mut writer)
            }
        }
    }

    pub fn try_to_vec(&self) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        self.serialize(&mut data)?;
        Ok(data)
    }
}

// Instruction discriminators (8-byte identifiers for each instruction)
pub const BUY_EXACT_IN_IX_DISCM: [u8; 8] = [250, 234, 13, 123, 213, 156, 19, 236];
pub const BUY_EXACT_OUT_IX_DISCM: [u8; 8] = [24, 211, 116, 40, 105, 3, 153, 56];
pub const INITIALIZE_IX_DISCM: [u8; 8] = [175, 175, 109, 31, 13, 152, 155, 237];
pub const SELL_EXACT_IN_IX_DISCM: [u8; 8] = [149, 39, 222, 155, 211, 124, 152, 26];
pub const SELL_EXACT_OUT_IX_DISCM: [u8; 8] = [95, 200, 71, 34, 8, 9, 11, 166];

// ========== INITIALIZE INSTRUCTION (POOL CREATION) ==========
// This is the most important instruction for monitoring new token creation

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InitializeIxArgs {
    pub base_mint_param: MintParams,
    pub curve_param: CurveParams,
    pub vesting_param: VestingParams,
}

// ========== TRADING INSTRUCTIONS ==========

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuyExactInIxArgs {
    pub amount_in: u64,
    pub minimum_amount_out: u64,
    pub share_fee_rate: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuyExactOutIxArgs {
    pub amount_out: u64,
    pub maximum_amount_in: u64,
    pub share_fee_rate: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SellExactInIxArgs {
    pub amount_in: u64,
    pub minimum_amount_out: u64,
    pub share_fee_rate: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SellExactOutIxArgs {
    pub amount_out: u64,
    pub maximum_amount_in: u64,
    pub share_fee_rate: u64,
}

// ========== UTILITY FUNCTIONS FOR TOKEN MONITORING ==========

impl RaydiumLaunchpadProgramIx {
    /// Returns true if this instruction creates a new pool (new token launch)
    pub fn is_pool_creation(&self) -> bool {
        matches!(self, Self::Initialize(_))
    }

    /// Returns true if this is a trading instruction
    pub fn is_trade(&self) -> bool {
        matches!(
            self,
            Self::BuyExactIn(_) | Self::BuyExactOut(_) | Self::SellExactIn(_) | Self::SellExactOut(_)
        )
    }

    /// Extracts token information from Initialize instruction
    pub fn get_token_info(&self) -> Option<&MintParams> {
        match self {
            Self::Initialize(args) => Some(&args.base_mint_param),
            _ => None,
        }
    }
}