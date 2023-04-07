use std::path::PathBuf;

fn main() {
    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    println!("cargo:warning={:?}", out_path);

    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .build_transport(true)
        .compile(&["proto/nacos_grpc_service.proto"], &["proto"])
        .unwrap()
}
