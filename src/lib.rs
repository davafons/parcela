//! Parcela's Rust control plane. No proxy or supervisor is implemented here.
pub mod config;
pub mod context;
pub mod control;
pub mod discovery;
pub mod frontdoor;
pub mod http;
pub mod lifecycle;
pub mod owner;
pub mod private;
pub mod process;
pub mod registry;
pub mod router;
pub mod runtime;
pub mod sockets;
pub mod startup;
