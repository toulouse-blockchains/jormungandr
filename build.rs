extern crate protoc_rust_grpc;

use std::env;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    protoc_rust_grpc::run(protoc_rust_grpc::Args {
        out_dir: &out_dir,
        includes: &["proto"],
        input: &["proto/node.proto", "proto/types.proto"],
        rust_protobuf: true, // also generate protobuf messages, not just services
        ..Default::default()
    }).expect("protoc-rust-grpc");
}
