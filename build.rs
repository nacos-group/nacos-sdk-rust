use std::env;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    grpcio_compiler::prost_codegen::compile_protos(
        &["./proto/nacos_grpc_service.proto"],
        &["./proto"],
        out_dir.as_str(),
    )
    .unwrap();
}
