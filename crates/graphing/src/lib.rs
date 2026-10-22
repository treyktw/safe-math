//! Renderer-neutral graph specifications.
//!
//! This engine validates mathematical expressions with the same safe parser
//! used by the math engine, then emits a versioned specification. It does not
//! choose a frontend renderer and never evaluates arbitrary JavaScript.

#![forbid(unsafe_code)]

mod engine;
mod protocol;

pub use engine::{GraphingEngine, SpecificationGraphingEngine};
pub use protocol::{
    AxisScale, AxisSpec, GraphAxes, GraphDimension, GraphInteraction, GraphRequest, GraphSeries,
    GraphSeriesSpec, GraphSpec, LineStyle, Point2, Point3, Range, Viewport,
};
