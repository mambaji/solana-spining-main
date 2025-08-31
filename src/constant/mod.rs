pub mod seeds {
    pub const GLOBAL_SEED: &[u8] = b"global";

    pub const MINT_AUTHORITY_SEED: &[u8] = b"mint_authority";

    pub const BONDING_CURVE_SEED: &[u8] = b"bonding_curve";

    pub const METADATA_SEED: &[u8] = b"metadata";

    pub const CREATOR_VAULT_SEED: &[u8] = b"creator_vault";
}

pub mod accounts {
    use solana_sdk::{pubkey, Pubkey};

    pub const PUMPFUN: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
    
    pub const MPL_TOKEN_METADATA: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

    pub const EVENT_AUTHORITY: Pubkey = pubkey!("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1");

    pub const SYSTEM_PROGRAM: Pubkey = pubkey!("11111111111111111111111111111111");

    pub const TOKEN_PROGRAM: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

    pub const GLOBAL_VOLUME_ACCUMULATOR: Pubkey = pubkey!("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y");

    pub const ASSOCIATED_TOKEN_PROGRAM: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

    pub const RENT: Pubkey = pubkey!("SysvarRent111111111111111111111111111111111");
}