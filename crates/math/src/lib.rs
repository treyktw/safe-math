//! The mathematics engine contract and SymPy-backed implementation.
//!
//! Raw user expressions are parsed into [`Expression`] in Rust. The backend
//! receives only that structured, allowlisted tree; it never calls Python
//! `eval`, `sympify`, or `parse_expr` on user-controlled strings.

#![forbid(unsafe_code)]

mod ast;
mod client;
mod parser;
mod protocol;

pub use ast::{Constant, Equation, Expression, Function, MathLimits};
pub use client::{MathEngine, SympyMathConfig, SympyMathEngine};
pub use parser::{parse_equation, parse_expression};
pub use protocol::{
    Assumption, CalculusBounds, LimitDirection, MathOperation, MathRequest, MathResponse,
    MathResult, MathValue, Simplification, Solution, SolveDomain, Verification, VerificationStatus,
};
