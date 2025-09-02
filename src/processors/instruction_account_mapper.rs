use serde::{Deserialize, Serialize};
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountMetadata {
    pub name: String,
    #[serde(serialize_with = "crate::serialization::serialize_pubkey")]
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
    pub docs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlAccount {
    pub name: String,
    #[serde(default)]
    pub writable: Option<bool>,
    #[serde(default)]
    pub signer: Option<bool>,
    #[serde(default)]
    pub optional: Option<bool>,
    #[serde(default)]
    pub docs: Option<Vec<String>>,
    #[serde(default)]
    pub pda: Option<serde_json::Value>,
    #[serde(default)]
    pub relations: Option<Vec<String>>,
    #[serde(default)]
    pub address: Option<String>,
    // Legacy fields for backwards compatibility
    #[serde(alias = "isMut", default)]
    pub is_mut: Option<bool>,
    #[serde(alias = "isSigner", default)]
    pub is_signer: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlInstruction {
    pub name: String,
    pub accounts: Vec<IdlAccount>,
    pub args: Vec<IdlField>,
    pub docs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlField {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: IdlType,
    pub docs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IdlType {
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "u8")]
    U8,
    #[serde(rename = "i8")]
    I8,
    #[serde(rename = "u16")]
    U16,
    #[serde(rename = "i16")]
    I16,
    #[serde(rename = "u32")]
    U32,
    #[serde(rename = "i32")]
    I32,
    #[serde(rename = "f32")]
    F32,
    #[serde(rename = "u64")]
    U64,
    #[serde(rename = "i64")]
    I64,
    #[serde(rename = "f64")]
    F64,
    #[serde(rename = "u128")]
    U128,
    #[serde(rename = "i128")]
    I128,
    #[serde(rename = "u256")]
    U256,
    #[serde(rename = "i256")]
    I256,
    #[serde(rename = "bytes")]
    Bytes,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "publicKey")]
    PublicKey,
    #[serde(rename = "option")]
    Option(Box<IdlType>),
    #[serde(rename = "vec")]
    Vec(Box<IdlType>),
    #[serde(rename = "array")]
    Array(Box<IdlType>, usize),
    #[serde(rename = "defined")]
    Defined { name: String },
    #[serde(untagged)]
    DefinedString(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idl {
    pub address: Option<String>,
    pub metadata: IdlMetadata,
    pub instructions: Vec<IdlInstruction>,
    pub accounts: Option<Vec<IdlAccount>>,
    pub types: Option<Vec<IdlTypeDefinition>>,
    pub events: Option<Vec<IdlEvent>>,
    pub errors: Option<Vec<IdlErrorCode>>,
    pub constants: Option<Vec<IdlConstant>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlTypeDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: IdlTypeDefTy,
    pub docs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum IdlTypeDefTy {
    #[serde(rename = "struct")]
    Struct { 
        #[serde(deserialize_with = "deserialize_struct_fields")]
        fields: Vec<IdlField> 
    },
    #[serde(rename = "enum")]
    Enum { variants: Vec<IdlEnumVariant> },
}

fn deserialize_struct_fields<'de, D>(deserializer: D) -> Result<Vec<IdlField>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Deserialize;
    
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum FieldFormat {
        Full(Vec<IdlField>),
        Simple(Vec<String>),
    }
    
    match FieldFormat::deserialize(deserializer)? {
        FieldFormat::Full(fields) => Ok(fields),
        FieldFormat::Simple(field_names) => {
            Ok(field_names.into_iter().enumerate().map(|(i, name)| {
                if name == "bool" {
                    IdlField {
                        name: format!("field_{}", i),
                        type_: IdlType::Bool,
                        docs: None,
                    }
                } else {
                    // 处理其他基础类型
                    let type_ = match name.as_str() {
                        "u8" => IdlType::U8,
                        "i8" => IdlType::I8,
                        "u16" => IdlType::U16,
                        "i16" => IdlType::I16,
                        "u32" => IdlType::U32,
                        "i32" => IdlType::I32,
                        "u64" => IdlType::U64,
                        "i64" => IdlType::I64,
                        "f32" => IdlType::F32,
                        "f64" => IdlType::F64,
                        "u128" => IdlType::U128,
                        "i128" => IdlType::I128,
                        "string" => IdlType::String,
                        "publicKey" => IdlType::PublicKey,
                        "bytes" => IdlType::Bytes,
                        _ => IdlType::DefinedString(name.clone()),
                    };
                    IdlField {
                        name: format!("field_{}", i),
                        type_,
                        docs: None,
                    }
                }
            }).collect())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlEnumVariant {
    pub name: String,
    pub fields: Option<Vec<IdlField>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlEvent {
    pub name: String,
    #[serde(default)]
    pub discriminator: Option<Vec<u8>>,
    #[serde(default)]
    pub fields: Option<Vec<IdlField>>,
    #[serde(default)]
    pub r#type: Option<IdlEventType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlEventType {
    pub kind: String,
    pub fields: Vec<IdlEventField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlEventField {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: IdlType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlErrorCode {
    pub code: u32,
    pub name: String,
    pub msg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlConstant {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: IdlType,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlMetadata {
    pub name: String,
    pub version: String,
    pub spec: String,
    pub description: String,
    #[serde(default)]
    pub address: Option<String>,
}

pub trait InstructionAccountMapper {
    fn map_accounts(&self, account_metas: &[AccountMeta], instruction_name: &str) -> Result<Vec<AccountMetadata>>;
}

impl InstructionAccountMapper for Idl {
    fn map_accounts(&self, account_metas: &[AccountMeta], instruction_name: &str) -> Result<Vec<AccountMetadata>> {
        // 查找指定指令的IDL定义
        let instruction = self.instructions.iter()
            .find(|instr| instr.name == instruction_name)
            .ok_or_else(|| anyhow::anyhow!("Instruction '{}' not found in IDL", instruction_name))?;

        let mut mapped_accounts = Vec::new();
        
        // 映射账户元数据
        for (index, account_meta) in account_metas.iter().enumerate() {
            let account_name = if index < instruction.accounts.len() {
                instruction.accounts[index].name.clone()
            } else {
                format!("account_{}", index)
            };

            let docs = if index < instruction.accounts.len() {
                instruction.accounts[index].docs.clone()
            } else {
                None
            };

            mapped_accounts.push(AccountMetadata {
                name: account_name,
                pubkey: account_meta.pubkey,
                is_signer: account_meta.is_signer,
                is_writable: account_meta.is_writable,
                docs,
            });
        }

        Ok(mapped_accounts)
    }
}