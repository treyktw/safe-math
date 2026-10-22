use crate::ast::{Expression, MathLimits};
use crate::parser::{parse_equation_with_limits, parse_expression_with_limits};
use crate::protocol::{
    MathOperation, MathRequest, MathResponse, PreparedBounds, PreparedMathRequest,
    PreparedOperation, PreparedVerification, Verification, WorkerError,
};
use async_trait::async_trait;
use engine_core::{
    CostClass, Engine, EngineError, EngineHealth, EngineId, EngineKind, EngineMetadata,
    EngineResult, EngineRuntime, HealthStatus,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const MAX_WORKER_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

#[async_trait]
pub trait MathEngine: Engine {
    async fn execute(&self, request: MathRequest) -> EngineResult<MathResponse>;
}

#[derive(Debug, Clone)]
pub struct SympyMathConfig {
    /// Base URL of the isolated worker, e.g. `http://math:8079`.
    pub endpoint: String,
    /// HTTP ceiling. The worker has its own shorter per-operation kill timer;
    /// this outer timeout handles a wedged or unreachable container.
    pub timeout: Duration,
    pub limits: MathLimits,
}

impl Default for SympyMathConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8079".into(),
            timeout: Duration::from_secs(20),
            limits: MathLimits::default(),
        }
    }
}

#[derive(Clone)]
pub struct SympyMathEngine {
    client: reqwest::Client,
    config: SympyMathConfig,
    metadata: EngineMetadata,
}

impl SympyMathEngine {
    pub fn new(config: SympyMathConfig) -> EngineResult<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(config.timeout)
            .user_agent("safe-math/0.1 math-engine")
            .build()
            .map_err(|error| EngineError::dependency("math_client_failed", error.to_string()))?;
        let mut metadata = EngineMetadata::new(
            EngineId::new("math.sympy")?,
            EngineKind::Math,
            "SymPy mathematics engine",
        )
        .with_capabilities([
            "numerical_calculation",
            "symbolic_simplification",
            "factoring",
            "expansion",
            "differentiation",
            "integration",
            "limits",
            "equation_solving",
            "symbolic_verification",
            "latex_output",
        ]);
        metadata.runtime = EngineRuntime::Container;
        metadata.cost_class = CostClass::Low;
        metadata.concurrency_limit = Some(2);
        metadata
            .attributes
            .insert("inputFormat".into(), "math_ast_v1".into());
        metadata
            .attributes
            .insert("rawPythonEvaluation".into(), false.into());
        Ok(Self {
            client,
            config,
            metadata,
        })
    }

    fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.config.endpoint.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn prepare(&self, request: MathRequest) -> EngineResult<PreparedMathRequest> {
        validate_assumptions(&request.assumptions, self.config.limits)?;
        let operation = match request.operation {
            MathOperation::Calculate {
                expression,
                variables,
                precision,
            } => {
                if !(2..=self.config.limits.max_precision).contains(&precision) {
                    return Err(EngineError::invalid_input(
                        "math_precision_invalid",
                        format!(
                            "precision must be between 2 and {} digits",
                            self.config.limits.max_precision
                        ),
                    ));
                }
                PreparedOperation::Calculate {
                    expression: self.expression(&expression)?,
                    variables: self.values(variables)?,
                    precision,
                }
            }
            MathOperation::Simplify { expression, method } => PreparedOperation::Simplify {
                expression: self.expression(&expression)?,
                method,
            },
            MathOperation::Differentiate {
                expression,
                variable,
                order,
            } => {
                self.validate_order(order)?;
                PreparedOperation::Differentiate {
                    expression: self.expression(&expression)?,
                    variable: variable_name(&variable, self.config.limits)?,
                    order,
                }
            }
            MathOperation::Integrate {
                expression,
                variable,
                bounds,
            } => PreparedOperation::Integrate {
                expression: self.expression(&expression)?,
                variable: variable_name(&variable, self.config.limits)?,
                bounds: bounds
                    .map(|bounds| {
                        Ok(PreparedBounds {
                            lower: self.expression(&bounds.lower)?,
                            upper: self.expression(&bounds.upper)?,
                        })
                    })
                    .transpose()?,
            },
            MathOperation::Limit {
                expression,
                variable,
                point,
                direction,
            } => PreparedOperation::Limit {
                expression: self.expression(&expression)?,
                variable: variable_name(&variable, self.config.limits)?,
                point: self.expression(&point)?,
                direction,
            },
            MathOperation::Solve {
                equations,
                variables,
                domain,
            } => {
                let equations = self.equations(equations)?;
                let variables = if variables.is_empty() {
                    equations
                        .iter()
                        .flat_map(|equation| equation.symbols())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                } else {
                    variables
                        .iter()
                        .map(|variable| variable_name(variable, self.config.limits))
                        .collect::<EngineResult<Vec<_>>>()?
                };
                if variables.is_empty() {
                    return Err(EngineError::invalid_input(
                        "math_solve_variables_missing",
                        "solving requires at least one variable",
                    ));
                }
                if variables.len() > self.config.limits.max_variables {
                    return Err(EngineError::capacity(
                        "math_too_many_variables",
                        format!(
                            "solve request exceeds {} variables",
                            self.config.limits.max_variables
                        ),
                    ));
                }
                PreparedOperation::Solve {
                    equations,
                    variables,
                    domain,
                }
            }
            MathOperation::Verify { assertion } => PreparedOperation::Verify {
                assertion: match assertion {
                    Verification::Equivalent { left, right } => PreparedVerification::Equivalent {
                        left: self.expression(&left)?,
                        right: self.expression(&right)?,
                    },
                    Verification::Satisfies { equations, values } => {
                        PreparedVerification::Satisfies {
                            equations: self.equations(equations)?,
                            values: self.values(values)?,
                        }
                    }
                    Verification::Derivative {
                        expression,
                        variable,
                        candidate,
                        order,
                    } => {
                        self.validate_order(order)?;
                        PreparedVerification::Derivative {
                            expression: self.expression(&expression)?,
                            variable: variable_name(&variable, self.config.limits)?,
                            candidate: self.expression(&candidate)?,
                            order,
                        }
                    }
                    Verification::Antiderivative {
                        expression,
                        variable,
                        candidate,
                    } => PreparedVerification::Antiderivative {
                        expression: self.expression(&expression)?,
                        variable: variable_name(&variable, self.config.limits)?,
                        candidate: self.expression(&candidate)?,
                    },
                },
            },
        };
        Ok(PreparedMathRequest {
            operation,
            assumptions: request.assumptions,
        })
    }

    fn expression(&self, input: &str) -> EngineResult<Expression> {
        parse_expression_with_limits(input, self.config.limits)
    }

    fn values(
        &self,
        values: BTreeMap<String, String>,
    ) -> EngineResult<BTreeMap<String, Expression>> {
        if values.len() > self.config.limits.max_variables {
            return Err(EngineError::capacity(
                "math_too_many_variables",
                format!(
                    "request exceeds {} variable values",
                    self.config.limits.max_variables
                ),
            ));
        }
        values
            .into_iter()
            .map(|(name, value)| {
                Ok((
                    variable_name(&name, self.config.limits)?,
                    self.expression(&value)?,
                ))
            })
            .collect()
    }

    fn equations(&self, equations: Vec<String>) -> EngineResult<Vec<crate::Equation>> {
        if equations.is_empty() {
            return Err(EngineError::invalid_input(
                "math_equations_missing",
                "at least one equation is required",
            ));
        }
        if equations.len() > self.config.limits.max_equations {
            return Err(EngineError::capacity(
                "math_too_many_equations",
                format!(
                    "request exceeds {} equations",
                    self.config.limits.max_equations
                ),
            ));
        }
        let equations = equations
            .iter()
            .map(|equation| parse_equation_with_limits(equation, self.config.limits))
            .collect::<EngineResult<Vec<_>>>()?;
        let nodes = equations
            .iter()
            .map(|equation| equation.left.node_count() + equation.right.node_count())
            .sum::<usize>();
        if nodes > self.config.limits.max_nodes {
            return Err(EngineError::capacity(
                "math_system_too_complex",
                format!(
                    "equation system exceeds the {} total node limit",
                    self.config.limits.max_nodes
                ),
            ));
        }
        Ok(equations)
    }

    fn validate_order(&self, order: u32) -> EngineResult<()> {
        if order == 0 || order > self.config.limits.max_derivative_order {
            return Err(EngineError::invalid_input(
                "math_derivative_order_invalid",
                format!(
                    "derivative order must be between 1 and {}",
                    self.config.limits.max_derivative_order
                ),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl Engine for SympyMathEngine {
    fn metadata(&self) -> EngineMetadata {
        self.metadata.clone()
    }

    async fn health(&self) -> EngineHealth {
        let started = Instant::now();
        let mut health = match tokio::time::timeout(
            Duration::from_secs(3),
            self.client.get(self.endpoint("health")).send(),
        )
        .await
        {
            Ok(Ok(response)) if response.status().is_success() => {
                EngineHealth::healthy(started.elapsed().as_millis() as u64)
            }
            Ok(Ok(response)) => {
                let mut health = EngineHealth::healthy(started.elapsed().as_millis() as u64);
                health.status = HealthStatus::Unhealthy;
                health.message = Some(format!("math worker returned {}", response.status()));
                health
            }
            Ok(Err(error)) => {
                let mut health = EngineHealth::healthy(started.elapsed().as_millis() as u64);
                health.status = HealthStatus::Unhealthy;
                health.message = Some(format!("math worker unreachable: {error}"));
                health
            }
            Err(_) => {
                let mut health = EngineHealth::healthy(started.elapsed().as_millis() as u64);
                health.status = HealthStatus::Unhealthy;
                health.message = Some("math worker health check timed out".into());
                health
            }
        };
        health
            .details
            .insert("endpoint".into(), self.config.endpoint.clone().into());
        health
    }
}

#[async_trait]
impl MathEngine for SympyMathEngine {
    async fn execute(&self, request: MathRequest) -> EngineResult<MathResponse> {
        let request = self.prepare(request)?;
        let response = self
            .client
            .post(self.endpoint("v1/math"))
            .json(&request)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    EngineError::timeout(
                        "math_worker_timeout",
                        format!(
                            "math operation exceeded {} seconds",
                            self.config.timeout.as_secs()
                        ),
                    )
                } else {
                    EngineError::unavailable(
                        "math_worker_unreachable",
                        format!("could not reach the math worker: {error}"),
                    )
                }
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_WORKER_RESPONSE_BYTES)
        {
            return Err(EngineError::dependency(
                "math_response_too_large",
                "math worker response exceeded 4 MiB",
            ));
        }
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|error| EngineError::dependency("math_response_broken", error.to_string()))?;
        if body.len() as u64 > MAX_WORKER_RESPONSE_BYTES {
            return Err(EngineError::dependency(
                "math_response_too_large",
                "math worker response exceeded 4 MiB",
            ));
        }
        if !status.is_success() {
            let error = serde_json::from_slice::<WorkerError>(&body).unwrap_or(WorkerError {
                code: "math_worker_failed".into(),
                message: String::from_utf8_lossy(&body).chars().take(500).collect(),
                retryable: status.is_server_error(),
            });
            return Err(if status.as_u16() == 408 {
                EngineError::timeout(error.code, error.message)
            } else if status.as_u16() == 429 {
                EngineError::rate_limited(error.code, error.message)
            } else if status.is_client_error() {
                EngineError::invalid_input(error.code, error.message)
            } else if error.retryable {
                EngineError::unavailable(error.code, error.message)
            } else {
                EngineError::dependency(error.code, error.message)
            });
        }
        serde_json::from_slice(&body).map_err(|error| {
            EngineError::dependency(
                "math_response_invalid",
                format!("math worker returned invalid JSON: {error}"),
            )
        })
    }
}

fn variable_name(value: &str, limits: MathLimits) -> EngineResult<String> {
    match parse_expression_with_limits(value.trim(), limits)? {
        Expression::Symbol { name } => Ok(name),
        _ => Err(EngineError::invalid_input(
            "math_variable_invalid",
            format!("'{value}' is not a variable name"),
        )),
    }
}

fn validate_assumptions(
    assumptions: &BTreeMap<String, BTreeSet<crate::protocol::Assumption>>,
    limits: MathLimits,
) -> EngineResult<()> {
    if assumptions.len() > limits.max_variables {
        return Err(EngineError::capacity(
            "math_too_many_variables",
            format!("assumptions exceed {} variables", limits.max_variables),
        ));
    }
    for variable in assumptions.keys() {
        variable_name(variable, limits)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{CalculusBounds, SolveDomain};

    fn engine() -> SympyMathEngine {
        SympyMathEngine::new(SympyMathConfig::default()).unwrap()
    }

    #[test]
    fn solve_infers_variables_from_equations_deterministically() {
        let request = MathRequest::new(MathOperation::Solve {
            equations: vec!["y = 2x + 1".into(), "x + y = 7".into()],
            variables: Vec::new(),
            domain: SolveDomain::Real,
        });
        let prepared = engine().prepare(request).unwrap();
        let PreparedOperation::Solve { variables, .. } = prepared.operation else {
            panic!("wrong operation")
        };
        assert_eq!(variables, vec!["x", "y"]);
    }

    #[test]
    fn every_raw_bound_and_substitution_is_parsed_into_an_ast() {
        let request = MathRequest::new(MathOperation::Integrate {
            expression: "x^2".into(),
            variable: "x".into(),
            bounds: Some(CalculusBounds {
                lower: "0".into(),
                upper: "pi".into(),
            }),
        });
        let prepared = serde_json::to_value(engine().prepare(request).unwrap()).unwrap();
        assert_eq!(prepared["operation"]["bounds"]["upper"]["kind"], "constant");
    }

    #[test]
    fn invalid_precision_order_and_variable_names_stop_before_http() {
        let calculate = MathRequest::new(MathOperation::Calculate {
            expression: "1/3".into(),
            variables: BTreeMap::new(),
            precision: 500,
        });
        assert!(engine().prepare(calculate).is_err());

        let derivative = MathRequest::new(MathOperation::Differentiate {
            expression: "x^2".into(),
            variable: "x + 1".into(),
            order: 1,
        });
        assert!(engine().prepare(derivative).is_err());
    }
}
