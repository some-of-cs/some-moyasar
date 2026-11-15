//! Tonic gRPC server example for `some-moyasar`.

#![forbid(unsafe_code)]

mod proto;
mod service;
mod status_mapper;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
