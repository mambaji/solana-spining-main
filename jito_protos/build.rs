use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 🔧 修复：跨平台protoc支持
    
    // 检查系统是否有protoc
    let has_protoc = Command::new("protoc")
        .arg("--version")
        .output()
        .is_ok();
    
    if !has_protoc {
        println!("cargo:warning=protoc not found in PATH. Please install protobuf-compiler:");
        println!("cargo:warning=  Ubuntu/Debian: sudo apt-get install protobuf-compiler");
        println!("cargo:warning=  CentOS/RHEL: sudo yum install protobuf-compiler");
        println!("cargo:warning=  Fedora: sudo dnf install protobuf-compiler");
        println!("cargo:warning=  macOS: brew install protobuf");
        println!("cargo:warning=  Or download from: https://github.com/protocolbuffers/protobuf/releases");
        
        // 尝试从环境变量获取protoc路径
        if let Ok(protoc_path) = std::env::var("PROTOC") {
            println!("cargo:warning=Using PROTOC from environment: {}", protoc_path);
        } else {
            println!("cargo:warning=You can also set PROTOC environment variable to protoc binary path");
        }
    }
    
    // 使用tonic-build编译proto文件
    tonic_build::configure()
        .compile(
            &[
                "protos/auth.proto",
                "protos/shared.proto", 
                "protos/shredstream.proto",
            ],
            &["protos"],
        )?;
    
    Ok(())
}
