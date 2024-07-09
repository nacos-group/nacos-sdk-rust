use std::io::Result;

fn main() -> Result<()> {
    tonic_build::configure()
        .build_server(false)
        .compile(&["proto/nacos_grpc_service.proto"], &["proto/"])?;

    Ok(())
}
