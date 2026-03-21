fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["../../proto/controller.proto"], &["../../proto"])?;

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["../../proto/node.proto"], &["../../proto"])?;

    Ok(())
}
