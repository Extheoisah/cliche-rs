//! Generate the versioned gRPC contract (requires protoc).
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/cliche/v1/cliche.proto");
    tonic_prost_build::configure().compile_protos(&["proto/cliche/v1/cliche.proto"], &["proto"])?;
    Ok(())
}
