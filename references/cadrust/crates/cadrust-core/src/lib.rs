//! cadrust-core: the Model layer. A document is a list of parametric features
//! (the timeline) plus named parameters. `evaluate` replays the timeline on
//! the OpenCascade kernel (via cadrum) and returns the resulting bodies.

pub mod b64;
pub mod doc;
pub mod eval;
pub mod expr;
pub mod graph;
pub mod sketch;
pub mod naming;

pub use cadrum::{DVec3, Solid};
pub use doc::{Document, Feature, Plane, PlaneRef, FaceRef, SketchEntity, BoolOp, EdgeSelect, NamedSelect, Constraint};
pub use eval::{evaluate, evaluate_with, Evaluated, BodyInfo, EdgeInfo, SketchInfo};
pub use graph::EvalCache;
pub use expr::Expr;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("feature {feature}: {message}")]
    Feature { feature: String, message: String },
    #[error("expression '{0}': {1}")]
    Expr(String, String),
    #[error("kernel: {0}")]
    Kernel(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<cadrum::Error> for Error {
    fn from(e: cadrum::Error) -> Self {
        Error::Kernel(format!("{e:?}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
