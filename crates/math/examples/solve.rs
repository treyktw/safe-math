//! Solve one equation end to end: Rust parser -> typed AST -> SymPy worker.
//!
//! ```bash
//! docker compose up -d math
//! cargo run -p engine-math --example solve -- "x^2 - 5x + 6 = 0"
//! cargo run -p engine-math --example solve -- --ast "2x + 3(x - 1) = 7"
//! ```
//!
//! Input that the parser refuses never leaves this process, so
//! `cargo run -p engine-math --example solve -- "__import__('os')"` fails
//! here with a typed error and the worker never sees a byte of it.

use engine_math::{
    MathEngine, MathOperation, MathRequest, MathResult, SolveDomain, SympyMathConfig,
    SympyMathEngine, parse_equation,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut show_ast = false;
    let mut real_only = false;
    let mut equation_text = None;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--ast" => show_ast = true,
            "--real" => real_only = true,
            _ => equation_text = Some(argument),
        }
    }
    let Some(equation_text) = equation_text else {
        eprintln!("usage: solve [--ast] [--real] \"<equation>\"");
        std::process::exit(2);
    };

    // Parsed here first, purely to show what crosses the boundary. The engine
    // parses again inside `execute`; nothing else is sent.
    let equation = match parse_equation(&equation_text) {
        Ok(equation) => equation,
        Err(error) => {
            eprintln!("rejected before the worker: {error}");
            std::process::exit(1);
        }
    };
    println!("parsed:    {equation}");
    println!(
        "size:      {} nodes, depth {}, symbols {:?}",
        equation.left.node_count() + equation.right.node_count(),
        equation.left.depth().max(equation.right.depth()),
        equation.symbols()
    );
    if show_ast {
        println!("wire AST:  {}", serde_json::to_string_pretty(&equation)?);
    }

    let mut config = SympyMathConfig::default();
    if let Ok(endpoint) = std::env::var("MATH_ENDPOINT") {
        config.endpoint = endpoint;
    }
    let engine = SympyMathEngine::new(config)?;
    let response = engine
        .execute(MathRequest::new(MathOperation::Solve {
            equations: vec![equation_text],
            variables: Vec::new(),
            domain: if real_only {
                SolveDomain::Real
            } else {
                SolveDomain::Complex
            },
        }))
        .await?;

    match response.result {
        MathResult::Solutions {
            solutions,
            solution_set,
        } => {
            if solutions.is_empty() && solution_set.is_none() {
                println!("no solutions");
            }
            if let Some(set) = solution_set {
                println!("solution set: {}   (LaTeX: {})", set.text, set.latex);
            }
            for (index, solution) in solutions.iter().enumerate() {
                let assignments = solution
                    .assignments
                    .iter()
                    .map(|(name, value)| format!("{name} = {}", value.text))
                    .collect::<Vec<_>>()
                    .join(", ");
                let latex = solution
                    .assignments
                    .iter()
                    .map(|(name, value)| format!("{name} = {}", value.latex))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("solution {}: {assignments}   (LaTeX: {latex})", index + 1);
            }
        }
        other => println!("{}", serde_json::to_string_pretty(&other)?),
    }
    for warning in response.warnings {
        println!("warning:   {warning}");
    }
    println!(
        "backend:   {} in {} ms",
        response.backend, response.duration_ms
    );
    Ok(())
}
