use tonic_build::configure;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    configure()
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
