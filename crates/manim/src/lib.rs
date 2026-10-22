//! Structured Manim scene plans and a replaceable remote renderer boundary.
//!
//! The public protocol contains no Python source and no raw LaTeX. Mathematical
//! objects use the math crate's validated AST, preventing scene generation from becoming
//! an arbitrary code execution path.

#![forbid(unsafe_code)]

mod client;
mod protocol;
mod validation;

pub use client::{ManimEngine, RemoteManimConfig, RemoteManimEngine};
pub use protocol::{
    AnimationAction, AnimationStep, CameraFrame, Easing, ManimObject, ManimRenderRequest,
    ManimScene, ManimStyle, Position, RenderArtifact, RenderOutput, RenderSubmission, ShapeKind,
};
// Re-exported so a caller can check health without also depending on the
// engine core crate for one trait.
pub use engine_core::Engine;
pub use validation::validate_request;
