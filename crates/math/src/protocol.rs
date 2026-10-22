use crate::ast::{Equation, Expression};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Assumption {
    Real,
    Complex,
    Integer,
    Rational,
    Positive,
    Nonnegative,
    Nonzero,
    Finite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathRequest {
    pub operation: MathOperation,
    #[serde(default)]
    pub assumptions: BTreeMap<String, BTreeSet<Assumption>>,
}

impl MathRequest {
    pub fn new(operation: MathOperation) -> Self {
        Self {
            operation,
            assumptions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MathOperation {
    Calculate {
        expression: String,
        #[serde(default)]
        variables: BTreeMap<String, String>,
        #[serde(default = "default_precision")]
        precision: u32,
    },
    Simplify {
        expression: String,
        #[serde(default)]
        method: Simplification,
    },
    Differentiate {
        expression: String,
        variable: String,
        #[serde(default = "default_order")]
        order: u32,
    },
    Integrate {
        expression: String,
        variable: String,
        bounds: Option<CalculusBounds>,
    },
    Limit {
        expression: String,
        variable: String,
        point: String,
        #[serde(default)]
        direction: LimitDirection,
    },
    Solve {
        equations: Vec<String>,
        variables: Vec<String>,
        #[serde(default)]
        domain: SolveDomain,
    },
    Verify {
        assertion: Verification,
    },
}

fn default_precision() -> u32 {
    50
}

fn default_order() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Simplification {
    #[default]
    Smart,
    Factor,
    Expand,
    Cancel,
    Together,
    Trigonometric,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculusBounds {
    pub lower: String,
    pub upper: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitDirection {
    #[default]
    Both,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveDomain {
    #[default]
    Complex,
    Real,
    Integer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verification {
    Equivalent {
        left: String,
        right: String,
    },
    Satisfies {
        equations: Vec<String>,
        values: BTreeMap<String, String>,
    },
    Derivative {
        expression: String,
        variable: String,
        candidate: String,
        #[serde(default = "default_order")]
        order: u32,
    },
    Antiderivative {
        expression: String,
        variable: String,
        candidate: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathValue {
    /// Plain-text form. Ordinary expression results use this crate's syntax and can
    /// be fed into another engine call; non-expression sets may be display-only.
    pub text: String,
    /// Presentation form for KaTeX/MathLive.
    pub latex: String,
    pub approximate: Option<String>,
    #[serde(default)]
    pub free_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    pub assignments: BTreeMap<String, MathValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Proven,
    Disproven,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MathResult {
    Value {
        value: MathValue,
    },
    Solutions {
        #[serde(default)]
        solutions: Vec<Solution>,
        /// Non-finite sets such as an interval or ImageSet are represented
        /// here instead of being truncated into a misleading finite list.
        ///
        /// Renamed explicitly: `rename_all` on the enum renames variants, not
        /// the fields inside them, and the worker sends camelCase. Without
        /// this the field silently decodes as `None` and `sin(x) = 0` reads as
        /// "no solutions".
        #[serde(default, rename = "solutionSet")]
        solution_set: Option<MathValue>,
    },
    Verification {
        status: VerificationStatus,
        /// `None` means the assertion was mathematically inconclusive.
        valid: Option<bool>,
        difference: Option<MathValue>,
        #[serde(default)]
        counterexample: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathResponse {
    pub result: MathResult,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub backend: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedMathRequest {
    pub operation: PreparedOperation,
    pub assumptions: BTreeMap<String, BTreeSet<Assumption>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(crate) enum PreparedOperation {
    Calculate {
        expression: Expression,
        variables: BTreeMap<String, Expression>,
        precision: u32,
    },
    Simplify {
        expression: Expression,
        method: Simplification,
    },
    Differentiate {
        expression: Expression,
        variable: String,
        order: u32,
    },
    Integrate {
        expression: Expression,
        variable: String,
        bounds: Option<PreparedBounds>,
    },
    Limit {
        expression: Expression,
        variable: String,
        point: Expression,
        direction: LimitDirection,
    },
    Solve {
        equations: Vec<Equation>,
        variables: Vec<String>,
        domain: SolveDomain,
    },
    Verify {
        assertion: PreparedVerification,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedBounds {
    pub lower: Expression,
    pub upper: Expression,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PreparedVerification {
    Equivalent {
        left: Expression,
        right: Expression,
    },
    Satisfies {
        equations: Vec<Equation>,
        values: BTreeMap<String, Expression>,
    },
    Derivative {
        expression: Expression,
        variable: String,
        candidate: Expression,
        order: u32,
    },
    Antiderivative {
        expression: Expression,
        variable: String,
        candidate: Expression,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkerError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from the SymPy worker for `sin(x) = 0` over the reals. The
    /// wire format is the contract between two languages, so it is tested
    /// against what the other side actually said.
    const INFINITE_SOLUTION_SET: &str = r#"{"result":{"kind":"solutions","solutions":[],"solutionSet":{"text":"Union(ImageSet(Lambda(_n, 2*_n*pi), Integers), ImageSet(Lambda(_n, 2*_n*pi + pi), Integers))","latex":"\\left\\{2 n \\pi\\; \\middle|\\; n \\in \\mathbb{Z}\\right\\} \\cup \\left\\{2 n \\pi + \\pi\\; \\middle|\\; n \\in \\mathbb{Z}\\right\\}","approximate":null,"freeSymbols":[]}},"warnings":[],"backend":"sympy-1.14.0","durationMs":47}"#;

    #[test]
    fn an_infinite_solution_set_survives_decoding() {
        let response: MathResponse = serde_json::from_str(INFINITE_SOLUTION_SET).unwrap();
        let MathResult::Solutions {
            solutions,
            solution_set,
        } = response.result
        else {
            panic!("wrong result kind")
        };
        assert!(solutions.is_empty());
        let set = solution_set.expect("the worker sent a solution set");
        assert!(set.text.starts_with("Union(ImageSet"));
        assert_eq!(response.duration_ms, 47);
    }

    #[test]
    fn a_verification_reads_its_counterexample() {
        let response: MathResponse = serde_json::from_str(
            r#"{"result":{"kind":"verification","status":"disproven","valid":false,"difference":null,"counterexample":{"x":"1"}},"warnings":[],"backend":"sympy-1.14.0","durationMs":3}"#,
        )
        .unwrap();
        let MathResult::Verification {
            status,
            counterexample,
            ..
        } = response.result
        else {
            panic!("wrong result kind")
        };
        assert_eq!(status, VerificationStatus::Disproven);
        assert_eq!(counterexample.get("x").map(String::as_str), Some("1"));
    }
}
