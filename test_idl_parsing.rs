use solana_spining::processors::instruction_account_mapper::Idl;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("测试IDL解析...");
    
    // 尝试加载PumpFun IDL
    if let Ok(idl_content) = fs::read_to_string("idls/pumpfun_0.1.0.json") {
        match serde_json::from_str::<Idl>(&idl_content) {
            Ok(idl) => {
                println!("✅ 成功解析PumpFun IDL");
                println!("   - 指令数量: {}", idl.instructions.len());
                println!("   - 账户类型数量: {}", idl.accounts.as_ref().map_or(0, |a| a.len()));
                println!("   - 自定义类型数量: {}", idl.types.as_ref().map_or(0, |t| t.len()));
                
                // 检查OptionBool类型是否正确解析
                if let Some(types) = &idl.types {
                    if let Some(option_bool_type) = types.iter().find(|t| t.name == "OptionBool") {
                        println!("   - 找到OptionBool类型定义: {:?}", option_bool_type);
                    }
                }
            }
            Err(e) => {
                println!("❌ 解析PumpFun IDL失败: {}", e);
                return Err(e.into());
            }
        }
    } else {
        println!("⚠️  未找到PumpFun IDL文件");
    }
    
    Ok(())
}