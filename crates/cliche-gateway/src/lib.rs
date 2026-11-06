//! Embeddable gRPC adapter. The WebSocket client remains independently usable.
/// Generated cliche.v1 protobuf client/server types.
#[allow(missing_docs)]
pub mod proto {
    tonic::include_proto!("cliche.v1");
}
mod service;
pub use service::{Gateway, GatewayPolicy};
