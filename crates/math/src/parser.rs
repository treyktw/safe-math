use crate::ast::{Constant, Equation, Expression, Function, MathLimits};
use engine_core::{EngineError, EngineResult};

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(String),
    Identifier(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Bang,
    LeftParen,
    RightParen,
    Comma,
    Equal,
}

pub fn parse_expression(input: &str) -> EngineResult<Expression> {
    parse_expression_with_limits(input, MathLimits::default())
}

pub fn parse_equation(input: &str) -> EngineResult<Equation> {
    parse_equation_with_limits(input, MathLimits::default())
}

pub(crate) fn parse_expression_with_limits(
    input: &str,
    limits: MathLimits,
) -> EngineResult<Expression> {
    let tokens = lex(input, limits)?;
    if tokens.iter().any(|token| matches!(token, Token::Equal)) {
        return Err(EngineError::invalid_input(
            "math_expression_contains_equation",
            "an expression cannot contain '='; use an equation operation",
        ));
    }
    parse_tokens(&tokens, limits)
}

pub(crate) fn parse_equation_with_limits(
    input: &str,
    limits: MathLimits,
) -> EngineResult<Equation> {
    let tokens = lex(input, limits)?;
    let equals = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| matches!(token, Token::Equal).then_some(index))
        .collect::<Vec<_>>();
    let equation = match equals.as_slice() {
        [] => Equation {
            left: parse_tokens(&tokens, limits)?,
            right: Expression::Integer { value: "0".into() },
        },
        [index] if *index > 0 && *index + 1 < tokens.len() => Equation {
            left: parse_tokens(&tokens[..*index], limits)?,
            right: parse_tokens(&tokens[*index + 1..], limits)?,
        },
        [_] => {
            return Err(EngineError::invalid_input(
                "math_equation_side_missing",
                "both sides of an equation must contain an expression",
            ));
        }
        _ => {
            return Err(EngineError::invalid_input(
                "math_equation_invalid",
                "an equation may contain exactly one '='",
            ));
        }
    };
    equation.validate(limits)?;
    Ok(equation)
}

fn parse_tokens(tokens: &[Token], limits: MathLimits) -> EngineResult<Expression> {
    if tokens.is_empty() {
        return Err(EngineError::invalid_input(
            "math_expression_empty",
            "math expression must not be empty",
        ));
    }
    let mut parser = Parser {
        tokens,
        cursor: 0,
        limits,
        recursion_depth: 0,
    };
    let expression = parser.expression(0)?;
    if parser.cursor != tokens.len() {
        return Err(parser.unexpected("end of expression"));
    }
    expression.validate(limits)?;
    Ok(expression)
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
    limits: MathLimits,
    recursion_depth: usize,
}

impl Parser<'_> {
    fn expression(&mut self, minimum_binding_power: u8) -> EngineResult<Expression> {
        self.recursion_depth += 1;
        if self.recursion_depth > self.limits.max_depth {
            self.recursion_depth -= 1;
            return Err(EngineError::capacity(
                "math_expression_too_deep",
                format!("parser recursion exceeds {} levels", self.limits.max_depth),
            ));
        }
        let result = self.expression_inner(minimum_binding_power);
        self.recursion_depth -= 1;
        result
    }

    fn expression_inner(&mut self, minimum_binding_power: u8) -> EngineResult<Expression> {
        let mut left = self.prefix()?;

        loop {
            if self.peek_is(&Token::Bang) {
                if 40 < minimum_binding_power {
                    break;
                }
                self.cursor += 1;
                left = Expression::Function {
                    function: Function::Factorial,
                    arguments: vec![left],
                };
                continue;
            }

            let (left_binding, right_binding, operator, implicit) = match self.peek() {
                Some(Token::Plus) => (10, 11, Binary::Add, false),
                Some(Token::Minus) => (10, 11, Binary::Subtract, false),
                Some(Token::Star) => (20, 21, Binary::Multiply, false),
                Some(Token::Slash) => (20, 21, Binary::Divide, false),
                Some(Token::Caret) => (30, 30, Binary::Power, false),
                Some(token) if starts_primary(token) => (20, 21, Binary::Multiply, true),
                _ => break,
            };
            if left_binding < minimum_binding_power {
                break;
            }
            if !implicit {
                self.cursor += 1;
            }
            let right = self.expression(right_binding)?;
            left = combine(operator, left, right);
            if left.node_count() > self.limits.max_nodes {
                return Err(EngineError::capacity(
                    "math_expression_too_complex",
                    format!(
                        "expression exceeds the {} node limit",
                        self.limits.max_nodes
                    ),
                ));
            }
        }
        Ok(left)
    }

    fn prefix(&mut self) -> EngineResult<Expression> {
        match self.next() {
            Some(Token::Plus) => self.expression(25),
            Some(Token::Minus) => Ok(Expression::Negate {
                // Power binds more tightly than unary minus: -x^2 is -(x^2).
                value: Box::new(self.expression(25)?),
            }),
            Some(Token::Number(value)) => Ok(number_expression(value)),
            Some(Token::Identifier(name)) => self.identifier(name),
            Some(Token::LeftParen) => {
                let inner = self.expression(0)?;
                self.expect(Token::RightParen, "')'")?;
                Ok(inner)
            }
            Some(token) => Err(EngineError::invalid_input(
                "math_expression_syntax",
                format!("unexpected token {token:?} at position {}", self.cursor),
            )),
            None => Err(EngineError::invalid_input(
                "math_expression_incomplete",
                "math expression ended before a value was found",
            )),
        }
    }

    fn identifier(&mut self, name: String) -> EngineResult<Expression> {
        if let Some(function) = Function::parse(&name)
            && self.peek_is(&Token::LeftParen)
        {
            self.cursor += 1;
            let mut arguments = Vec::new();
            if !self.peek_is(&Token::RightParen) {
                loop {
                    arguments.push(self.expression(0)?);
                    if self.peek_is(&Token::Comma) {
                        self.cursor += 1;
                        continue;
                    }
                    break;
                }
            }
            self.expect(Token::RightParen, "')' after function arguments")?;
            function.validate_arity(arguments.len())?;
            return Ok(Expression::Function {
                function,
                arguments,
            });
        }
        Ok(match Constant::parse(&name) {
            Some(value) => Expression::Constant { value },
            None => Expression::Symbol { name },
        })
    }

    fn expect(&mut self, expected: Token, description: &str) -> EngineResult<()> {
        if self.peek_is(&expected) {
            self.cursor += 1;
            Ok(())
        } else {
            Err(self.unexpected(description))
        }
    }

    fn unexpected(&self, expected: &str) -> EngineError {
        EngineError::invalid_input(
            "math_expression_syntax",
            format!(
                "expected {expected} at token {}, found {:?}",
                self.cursor,
                self.peek()
            ),
        )
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.cursor).cloned();
        self.cursor += usize::from(token.is_some());
        token
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    fn peek_is(&self, expected: &Token) -> bool {
        self.peek() == Some(expected)
    }
}

#[derive(Clone, Copy)]
enum Binary {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
}

fn combine(operator: Binary, left: Expression, right: Expression) -> Expression {
    match operator {
        Binary::Add => match left {
            Expression::Add { mut terms } => {
                terms.push(right);
                Expression::Add { terms }
            }
            left => Expression::Add {
                terms: vec![left, right],
            },
        },
        Binary::Subtract => {
            let right = Expression::Negate {
                value: Box::new(right),
            };
            combine(Binary::Add, left, right)
        }
        Binary::Multiply => match left {
            Expression::Multiply { mut factors } => {
                factors.push(right);
                Expression::Multiply { factors }
            }
            left => Expression::Multiply {
                factors: vec![left, right],
            },
        },
        Binary::Divide => Expression::Divide {
            numerator: Box::new(left),
            denominator: Box::new(right),
        },
        Binary::Power => Expression::Power {
            base: Box::new(left),
            exponent: Box::new(right),
        },
    }
}

fn starts_primary(token: &Token) -> bool {
    matches!(
        token,
        Token::Number(_) | Token::Identifier(_) | Token::LeftParen
    )
}

fn number_expression(value: String) -> Expression {
    if value.contains(['.', 'e', 'E']) {
        Expression::Decimal { value }
    } else {
        Expression::Integer { value }
    }
}

fn lex(input: &str, limits: MathLimits) -> EngineResult<Vec<Token>> {
    let input = input.trim();
    if input.is_empty() {
        return Err(EngineError::invalid_input(
            "math_expression_empty",
            "math expression must not be empty",
        ));
    }
    if input.len() > limits.max_input_bytes {
        return Err(EngineError::capacity(
            "math_input_too_large",
            format!("math input exceeds {} bytes", limits.max_input_bytes),
        ));
    }

    let characters = input.char_indices().collect::<Vec<_>>();
    let mut cursor = 0;
    let mut tokens = Vec::new();
    while cursor < characters.len() {
        let (byte_index, character) = characters[cursor];
        if character.is_whitespace() {
            cursor += 1;
            continue;
        }
        let token = match character {
            '+' => Token::Plus,
            '-' | '−' => Token::Minus,
            '*' | '×' | '·' => Token::Star,
            '/' | '÷' => Token::Slash,
            '^' => Token::Caret,
            '!' => Token::Bang,
            '(' => Token::LeftParen,
            ')' => Token::RightParen,
            ',' => Token::Comma,
            '=' => Token::Equal,
            '∞' => Token::Identifier("infinity".into()),
            value if value.is_ascii_digit() || value == '.' => {
                let start = cursor;
                let mut saw_digit = value.is_ascii_digit();
                let mut saw_dot = value == '.';
                cursor += 1;
                while cursor < characters.len() {
                    let next = characters[cursor].1;
                    if next.is_ascii_digit() {
                        saw_digit = true;
                        cursor += 1;
                    } else if next == '.' && !saw_dot {
                        saw_dot = true;
                        cursor += 1;
                    } else {
                        break;
                    }
                }
                if !saw_digit {
                    return Err(EngineError::invalid_input(
                        "math_number_invalid",
                        format!("invalid number at byte {byte_index}"),
                    ));
                }
                if cursor < characters.len() && matches!(characters[cursor].1, 'e' | 'E') {
                    cursor += 1;
                    if cursor < characters.len() && matches!(characters[cursor].1, '+' | '-') {
                        cursor += 1;
                    }
                    let exponent_start = cursor;
                    while cursor < characters.len() && characters[cursor].1.is_ascii_digit() {
                        cursor += 1;
                    }
                    if exponent_start == cursor {
                        return Err(EngineError::invalid_input(
                            "math_number_invalid",
                            format!("scientific notation at byte {byte_index} has no exponent"),
                        ));
                    }
                }
                let end = characters
                    .get(cursor)
                    .map(|(index, _)| *index)
                    .unwrap_or(input.len());
                let value = input[characters[start].0..end].to_string();
                if value.len() > 256 {
                    return Err(EngineError::capacity(
                        "math_number_too_large",
                        "numeric literals may contain at most 256 characters",
                    ));
                }
                tokens.push(Token::Number(value));
                continue;
            }
            value if value.is_alphabetic() || value == '_' => {
                let start = cursor;
                cursor += 1;
                while cursor < characters.len()
                    && (characters[cursor].1.is_alphanumeric() || characters[cursor].1 == '_')
                {
                    cursor += 1;
                }
                let end = characters
                    .get(cursor)
                    .map(|(index, _)| *index)
                    .unwrap_or(input.len());
                let name = input[characters[start].0..end].to_string();
                if name.len() > 64 || name.contains("__") || name.starts_with('_') {
                    return Err(EngineError::invalid_input(
                        "math_symbol_invalid",
                        "symbol names must be at most 64 bytes and may not be private names",
                    ));
                }
                tokens.push(Token::Identifier(name));
                continue;
            }
            _ => {
                return Err(EngineError::invalid_input(
                    "math_character_unsupported",
                    format!("unsupported character '{character}' at byte {byte_index}"),
                ));
            }
        };
        tokens.push(token);
        cursor += 1;
        if tokens.len() > limits.max_tokens {
            return Err(EngineError::capacity(
                "math_too_many_tokens",
                format!("math input exceeds {} tokens", limits.max_tokens),
            ));
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_binds_tighter_than_unary_minus_and_is_right_associative() {
        assert_eq!(parse_expression("-x^2").unwrap().to_string(), "-x^2");
        assert_eq!(parse_expression("2^3^2").unwrap().to_string(), "2^3^2");
    }

    #[test]
    fn understands_implicit_multiplication_and_unicode_operators() {
        let expression = parse_expression("2x + 3(x − 1) × π").unwrap();
        assert_eq!(expression.to_string(), "2 * x + 3 * (x + -1) * pi");
        assert_eq!(expression.symbols(), ["x".to_string()].into());
    }

    #[test]
    fn parses_functions_arguments_and_factorials() {
        let expression = parse_expression("log(8, 2) + sin(x)! ").unwrap();
        assert_eq!(expression.to_string(), "log(8, 2) + factorial(sin(x))");
    }

    #[test]
    fn a_bare_expression_becomes_an_equation_to_zero() {
        let equation = parse_equation("x^2 - 4").unwrap();
        assert_eq!(equation.to_string(), "x^2 + -4 = 0");
    }

    #[test]
    fn rejects_multiple_equal_signs_and_private_symbols() {
        assert!(parse_equation("x = y = 2").is_err());
        assert!(parse_expression("__import__(x)").is_err());
    }

    #[test]
    fn limits_pathological_depth_before_the_worker() {
        let input = format!("{}x{}", "(".repeat(140), ")".repeat(140));
        assert!(parse_expression(&input).is_err());
    }
}
