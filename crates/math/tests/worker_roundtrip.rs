//! Round trips through a running SymPy worker.
//!
//! Skipped unless `WORKER_TESTS=1`, so `cargo test` passes on a machine with
//! no containers. To run them:
//!
//! ```bash
//! docker compose up -d math
//! WORKER_TESTS=1 cargo test -p engine-math --test worker_roundtrip
//! ```
//!
//! `MATH_ENDPOINT` overrides the default `http://127.0.0.1:8079`.

use engine_math::{
    MathEngine, MathOperation, MathRequest, MathResult, SolveDomain, SympyMathConfig,
    SympyMathEngine, Verification, VerificationStatus,
};

fn enabled() -> bool {
    if std::env::var("WORKER_TESTS").as_deref() == Ok("1") {
        return true;
    }
    eprintln!("skipped: set WORKER_TESTS=1 with the math worker running");
    false
}

fn endpoint() -> String {
    std::env::var("MATH_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8079".into())
}

fn engine() -> SympyMathEngine {
    SympyMathEngine::new(SympyMathConfig {
        endpoint: endpoint(),
        ..SympyMathConfig::default()
    })
    .unwrap()
}

#[tokio::test]
async fn solves_a_quadratic_through_the_worker() {
    if !enabled() {
        return;
    }
    let response = engine()
        .execute(MathRequest::new(MathOperation::Solve {
            equations: vec!["x^2 - 5x + 6 = 0".into()],
            variables: Vec::new(),
            domain: SolveDomain::Real,
        }))
        .await
        .unwrap();
    let MathResult::Solutions { solutions, .. } = response.result else {
        panic!("expected solutions")
    };
    let roots = solutions
        .iter()
        .map(|solution| solution.assignments["x"].text.clone())
        .collect::<Vec<_>>();
    assert_eq!(roots, ["2", "3"]);
}

#[tokio::test]
async fn an_infinite_solution_set_is_not_reported_as_no_solutions() {
    if !enabled() {
        return;
    }
    let response = engine()
        .execute(MathRequest::new(MathOperation::Solve {
            equations: vec!["sin(x) = 0".into()],
            variables: vec!["x".into()],
            domain: SolveDomain::Real,
        }))
        .await
        .unwrap();
    let MathResult::Solutions { solution_set, .. } = response.result else {
        panic!("expected solutions")
    };
    assert!(solution_set.is_some());
}

#[tokio::test]
async fn proves_a_derivative_and_disproves_a_cancelled_fraction() {
    if !enabled() {
        return;
    }
    let derivative = engine()
        .execute(MathRequest::new(MathOperation::Verify {
            assertion: Verification::Derivative {
                expression: "x^3".into(),
                variable: "x".into(),
                candidate: "3x^2".into(),
                order: 1,
            },
        }))
        .await
        .unwrap();
    assert!(matches!(
        derivative.result,
        MathResult::Verification {
            status: VerificationStatus::Proven,
            ..
        }
    ));

    // (x^2 - 1)/(x - 1) and x + 1 differ at x = 1, where the left side is
    // undefined. The worker has to say so rather than "simplify" it away.
    let fraction = engine()
        .execute(MathRequest::new(MathOperation::Verify {
            assertion: Verification::Equivalent {
                left: "(x^2 - 1) / (x - 1)".into(),
                right: "x + 1".into(),
            },
        }))
        .await
        .unwrap();
    let MathResult::Verification {
        status,
        counterexample,
        ..
    } = fraction.result
    else {
        panic!("expected a verification")
    };
    assert_eq!(status, VerificationStatus::Disproven);
    assert_eq!(counterexample["x"], "1");
}

/// The worker is reachable without going through Rust, so it must refuse a
/// hand-written payload that the Rust parser could never have produced.
#[tokio::test]
async fn the_worker_revalidates_a_payload_that_skipped_rust() {
    if !enabled() {
        return;
    }
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/math", endpoint()))
        .json(&serde_json::json!({
            "operation": {
                "operation": "simplify",
                "expression": { "kind": "python", "value": "__import__('os').system('id')" },
                "method": "smart"
            },
            "assumptions": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "math_ast_kind_invalid");

    let private_symbol = client
        .post(format!("{}/v1/math", endpoint()))
        .json(&serde_json::json!({
            "operation": {
                "operation": "simplify",
                "expression": { "kind": "symbol", "name": "__class__" },
                "method": "smart"
            },
            "assumptions": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(private_symbol.status().as_u16(), 400);
}
