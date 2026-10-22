//! Exercise the Rust parser, HTTP contract, isolated SymPy worker, and result
//! decoding together.
//!
//! ```bash
//! docker compose up -d math
//! cargo run -p engine-math --example smoke
//! ```

use engine_math::{
    MathEngine, MathOperation, MathRequest, Simplification, SolveDomain, SympyMathConfig,
    SympyMathEngine, Verification,
};
use std::collections::BTreeMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = SympyMathConfig::default();
    if let Ok(endpoint) = std::env::var("MATH_ENDPOINT") {
        config.endpoint = endpoint;
    }
    let engine = SympyMathEngine::new(config)?;
    let requests = [
        MathRequest::new(MathOperation::Calculate {
            expression: "sin(pi / 4)^2 + cos(pi / 4)^2".into(),
            variables: BTreeMap::new(),
            precision: 50,
        }),
        MathRequest::new(MathOperation::Simplify {
            expression: "(x^2 - 1) / (x - 1)".into(),
            method: Simplification::Cancel,
        }),
        MathRequest::new(MathOperation::Differentiate {
            expression: "x^3 + sin(x)".into(),
            variable: "x".into(),
            order: 1,
        }),
        MathRequest::new(MathOperation::Solve {
            equations: vec!["x^2 = 4".into()],
            variables: vec!["x".into()],
            domain: SolveDomain::Real,
        }),
        MathRequest::new(MathOperation::Verify {
            assertion: Verification::Derivative {
                expression: "x^3".into(),
                variable: "x".into(),
                candidate: "3x^2".into(),
                order: 1,
            },
        }),
    ];
    for request in requests {
        println!(
            "{}",
            serde_json::to_string_pretty(&engine.execute(request).await?)?
        );
    }
    Ok(())
}
