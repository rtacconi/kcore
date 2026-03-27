fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let proto_dir = manifest_dir.join("..").join("..").join("proto");
    let controller_proto = proto_dir.join("controller.proto");
    let node_proto = proto_dir.join("node.proto");

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[controller_proto], std::slice::from_ref(&proto_dir))?;

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&[node_proto], &[proto_dir])?;

    Ok(())
}
