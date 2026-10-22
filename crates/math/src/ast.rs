use engine_core::{EngineError, EngineResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// Complexity ceilings applied before any request reaches the symbolic worker.
/// They bound both parser work and the size of trees SymPy is allowed to build.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathLimits {
    pub max_input_bytes: usize,
    pub max_tokens: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_equations: usize,
    pub max_variables: usize,
    pub max_precision: u32,
    pub max_derivative_order: u32,
}

impl Default for MathLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 16 * 1024,
            max_tokens: 4_096,
            max_nodes: 2_048,
            max_depth: 128,
            max_equations: 32,
            max_variables: 64,
            max_precision: 200,
            max_derivative_order: 20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Constant {
    Pi,
    E,
    I,
    Infinity,
}

impl Constant {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "pi" | "π" => Some(Self::Pi),
            "e" | "ℯ" => Some(Self::E),
            "i" => Some(Self::I),
            "inf" | "infinity" | "oo" => Some(Self::Infinity),
            _ => None,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::E => "e",
            Self::I => "i",
            Self::Infinity => "infinity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Function {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Exp,
    Ln,
    Log,
    Sqrt,
    Abs,
    Floor,
    Ceiling,
    Factorial,
    Gamma,
    Min,
    Max,
}

impl Function {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "sin" => Some(Self::Sin),
            "cos" => Some(Self::Cos),
            "tan" => Some(Self::Tan),
            "asin" | "arcsin" => Some(Self::Asin),
            "acos" | "arccos" => Some(Self::Acos),
            "atan" | "arctan" => Some(Self::Atan),
            "sinh" => Some(Self::Sinh),
            "cosh" => Some(Self::Cosh),
            "tanh" => Some(Self::Tanh),
            "exp" => Some(Self::Exp),
            "ln" => Some(Self::Ln),
            "log" => Some(Self::Log),
            "sqrt" => Some(Self::Sqrt),
            "abs" => Some(Self::Abs),
            "floor" => Some(Self::Floor),
            "ceil" | "ceiling" => Some(Self::Ceiling),
            "factorial" => Some(Self::Factorial),
            "gamma" => Some(Self::Gamma),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    pub fn validate_arity(self, arity: usize) -> EngineResult<()> {
        let valid = match self {
            Self::Log => matches!(arity, 1 | 2),
            Self::Min | Self::Max => arity >= 1,
            _ => arity == 1,
        };
        if valid {
            Ok(())
        } else {
            Err(EngineError::invalid_input(
                "math_function_arity_invalid",
                format!("{} received {arity} arguments", self.text()),
            ))
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Tan => "tan",
            Self::Asin => "asin",
            Self::Acos => "acos",
            Self::Atan => "atan",
            Self::Sinh => "sinh",
            Self::Cosh => "cosh",
            Self::Tanh => "tanh",
            Self::Exp => "exp",
            Self::Ln => "ln",
            Self::Log => "log",
            Self::Sqrt => "sqrt",
            Self::Abs => "abs",
            Self::Floor => "floor",
            Self::Ceiling => "ceiling",
            Self::Factorial => "factorial",
            Self::Gamma => "gamma",
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

/// A deliberately small, serializable mathematical language.
///
/// N-ary addition and multiplication avoid pathological left-deep trees for
/// ordinary polynomials and make complexity accounting predictable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expression {
    Integer {
        value: String,
    },
    Decimal {
        value: String,
    },
    Symbol {
        name: String,
    },
    Constant {
        value: Constant,
    },
    Negate {
        value: Box<Expression>,
    },
    Add {
        terms: Vec<Expression>,
    },
    Multiply {
        factors: Vec<Expression>,
    },
    Divide {
        numerator: Box<Expression>,
        denominator: Box<Expression>,
    },
    Power {
        base: Box<Expression>,
        exponent: Box<Expression>,
    },
    Function {
        function: Function,
        arguments: Vec<Expression>,
    },
}

impl Expression {
    pub fn node_count(&self) -> usize {
        match self {
            Self::Integer { .. }
            | Self::Decimal { .. }
            | Self::Symbol { .. }
            | Self::Constant { .. } => 1,
            Self::Negate { value } => 1 + value.node_count(),
            Self::Add { terms } => 1 + terms.iter().map(Self::node_count).sum::<usize>(),
            Self::Multiply { factors } => 1 + factors.iter().map(Self::node_count).sum::<usize>(),
            Self::Divide {
                numerator,
                denominator,
            } => 1 + numerator.node_count() + denominator.node_count(),
            Self::Power { base, exponent } => 1 + base.node_count() + exponent.node_count(),
            Self::Function { arguments, .. } => {
                1 + arguments.iter().map(Self::node_count).sum::<usize>()
            }
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Self::Integer { .. }
            | Self::Decimal { .. }
            | Self::Symbol { .. }
            | Self::Constant { .. } => 1,
            Self::Negate { value } => 1 + value.depth(),
            Self::Add { terms } => 1 + terms.iter().map(Self::depth).max().unwrap_or(0),
            Self::Multiply { factors } => 1 + factors.iter().map(Self::depth).max().unwrap_or(0),
            Self::Divide {
                numerator,
                denominator,
            } => 1 + numerator.depth().max(denominator.depth()),
            Self::Power { base, exponent } => 1 + base.depth().max(exponent.depth()),
            Self::Function { arguments, .. } => {
                1 + arguments.iter().map(Self::depth).max().unwrap_or(0)
            }
        }
    }

    pub fn symbols(&self) -> BTreeSet<String> {
        let mut symbols = BTreeSet::new();
        self.collect_symbols(&mut symbols);
        symbols
    }

    fn collect_symbols(&self, symbols: &mut BTreeSet<String>) {
        match self {
            Self::Symbol { name } => {
                symbols.insert(name.clone());
            }
            Self::Negate { value } => value.collect_symbols(symbols),
            Self::Add { terms } => {
                for term in terms {
                    term.collect_symbols(symbols);
                }
            }
            Self::Multiply { factors } => {
                for factor in factors {
                    factor.collect_symbols(symbols);
                }
            }
            Self::Divide {
                numerator,
                denominator,
            } => {
                numerator.collect_symbols(symbols);
                denominator.collect_symbols(symbols);
            }
            Self::Power { base, exponent } => {
                base.collect_symbols(symbols);
                exponent.collect_symbols(symbols);
            }
            Self::Function { arguments, .. } => {
                for argument in arguments {
                    argument.collect_symbols(symbols);
                }
            }
            Self::Integer { .. } | Self::Decimal { .. } | Self::Constant { .. } => {}
        }
    }

    pub fn validate(&self, limits: MathLimits) -> EngineResult<()> {
        if self.node_count() > limits.max_nodes {
            return Err(EngineError::capacity(
                "math_expression_too_complex",
                format!("expression exceeds the {} node limit", limits.max_nodes),
            ));
        }
        if self.depth() > limits.max_depth {
            return Err(EngineError::capacity(
                "math_expression_too_deep",
                format!(
                    "expression exceeds the {} level depth limit",
                    limits.max_depth
                ),
            ));
        }
        if self.symbols().len() > limits.max_variables {
            return Err(EngineError::capacity(
                "math_too_many_variables",
                format!(
                    "expression exceeds the {} variable limit",
                    limits.max_variables
                ),
            ));
        }
        Ok(())
    }

    fn precedence(&self) -> u8 {
        match self {
            Self::Add { .. } => 10,
            Self::Multiply { .. } | Self::Divide { .. } => 20,
            Self::Negate { .. } => 25,
            Self::Power { .. } => 30,
            _ => 40,
        }
    }

    fn fmt_with_precedence(&self, f: &mut fmt::Formatter<'_>, parent: u8) -> fmt::Result {
        let own = self.precedence();
        let parentheses = own < parent;
        if parentheses {
            f.write_str("(")?;
        }
        match self {
            Self::Integer { value } | Self::Decimal { value } => f.write_str(value)?,
            Self::Symbol { name } => f.write_str(name)?,
            Self::Constant { value } => f.write_str(value.text())?,
            Self::Negate { value } => {
                f.write_str("-")?;
                value.fmt_with_precedence(f, own)?;
            }
            Self::Add { terms } => {
                for (index, term) in terms.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" + ")?;
                    }
                    term.fmt_with_precedence(f, own)?;
                }
            }
            Self::Multiply { factors } => {
                for (index, factor) in factors.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" * ")?;
                    }
                    factor.fmt_with_precedence(f, own)?;
                }
            }
            Self::Divide {
                numerator,
                denominator,
            } => {
                numerator.fmt_with_precedence(f, own)?;
                f.write_str(" / ")?;
                denominator.fmt_with_precedence(f, own + 1)?;
            }
            Self::Power { base, exponent } => {
                base.fmt_with_precedence(f, own)?;
                f.write_str("^")?;
                exponent.fmt_with_precedence(f, own)?;
            }
            Self::Function {
                function,
                arguments,
            } => {
                f.write_str(function.text())?;
                f.write_str("(")?;
                for (index, argument) in arguments.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    argument.fmt_with_precedence(f, 0)?;
                }
                f.write_str(")")?;
            }
        }
        if parentheses {
            f.write_str(")")?;
        }
        Ok(())
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_precedence(f, 0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Equation {
    pub left: Expression,
    pub right: Expression,
}

impl Equation {
    pub fn validate(&self, limits: MathLimits) -> EngineResult<()> {
        self.left.validate(limits)?;
        self.right.validate(limits)?;
        if self.left.node_count() + self.right.node_count() > limits.max_nodes {
            return Err(EngineError::capacity(
                "math_equation_too_complex",
                format!("equation exceeds the {} node limit", limits.max_nodes),
            ));
        }
        Ok(())
    }

    pub fn symbols(&self) -> BTreeSet<String> {
        self.left
            .symbols()
            .into_iter()
            .chain(self.right.symbols())
            .collect()
    }
}

impl fmt::Display for Equation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} = {}", self.left, self.right)
    }
}
