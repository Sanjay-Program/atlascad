//! Core creative abstractions: polygon meshes, Catmull-Clark subdivision, sculpting brushes,
//! UV unwrapping charts, and procedural geometry nodes for Atlas Create.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod geom_nodes;
pub mod mesh;
pub mod sculpt;
pub mod uv;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CreativeError {
    #[error("mesh error: {0}")]
    Mesh(String),
    #[error("sculpt error: {0}")]
    Sculpt(String),
    #[error("uv error: {0}")]
    Uv(String),
    #[error("geometry node error: {0}")]
    GeometryNode(String),
}
