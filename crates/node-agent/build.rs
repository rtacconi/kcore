fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let proto_dir = manifest_dir.join("..").join("..").join("proto");
    let node_proto = proto_dir.join("node.proto");
    let controller_proto = proto_dir.join("controller.proto");

    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(&[&node_proto], &[&proto_dir])?;

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&[&controller_proto], &[&proto_dir])?;

    Ok(())
}
