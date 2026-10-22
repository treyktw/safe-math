#!/usr/bin/env python3
"""Isolated SymPy worker for the structured mathematics protocol.

The HTTP boundary accepts only Math AST v1. It deliberately does not
expose sympify(), parse_expr(), eval(), arbitrary attributes, file paths, or
Python source. Rust performs the first validation pass; this process repeats
the structural checks because callers may reach the container directly.
"""

from __future__ import annotations

import json
import math
import multiprocessing
import os
import re
import threading
import time
from decimal import Decimal, InvalidOperation
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, cast

import sympy as sp  # pyright: ignore[reportMissingImports]

MAX_REQUEST_BYTES = int(os.getenv("MATH_MAX_REQUEST_BYTES", str(1024 * 1024)))
MAX_RESPONSE_BYTES = int(os.getenv("MATH_MAX_RESPONSE_BYTES", str(4 * 1024 * 1024)))
MAX_AST_NODES = int(os.getenv("MATH_MAX_AST_NODES", "2048"))
MAX_AST_DEPTH = int(os.getenv("MATH_MAX_AST_DEPTH", "128"))
OPERATION_TIMEOUT_SECONDS = int(os.getenv("MATH_OPERATION_TIMEOUT_SECONDS", "15"))
MAX_CONCURRENCY = int(os.getenv("MATH_MAX_CONCURRENCY", "2"))
PORT = int(os.getenv("MATH_PORT", "8079"))

NUMBER = re.compile(r"^[0-9]+(?:\.[0-9]*)?(?:[eE][+-]?[0-9]+)?$|^\.[0-9]+(?:[eE][+-]?[0-9]+)?$")
INTEGER = re.compile(r"^[0-9]+$")

FUNCTIONS = {
    "sin": sp.sin,
    "cos": sp.cos,
    "tan": sp.tan,
    "asin": sp.asin,
    "acos": sp.acos,
    "atan": sp.atan,
    "sinh": sp.sinh,
    "cosh": sp.cosh,
    "tanh": sp.tanh,
    "exp": sp.exp,
    "ln": sp.log,
    "log": sp.log,
    "sqrt": sp.sqrt,
    "abs": sp.Abs,
    "floor": sp.floor,
    "ceiling": sp.ceiling,
    "factorial": sp.factorial,
    "gamma": sp.gamma,
    "min": sp.Min,
    "max": sp.Max,
}

CONSTANTS = {
    "pi": sp.pi,
    "e": sp.E,
    "i": sp.I,
    "infinity": sp.oo,
}

ASSUMPTIONS = {
    "real": "real",
    "complex": "complex",
    "integer": "integer",
    "rational": "rational",
    "positive": "positive",
    "nonnegative": "nonnegative",
    "nonzero": "nonzero",
    "finite": "finite",
}


class MathWorkerError(Exception):
    def __init__(self, code: str, message: str, status: int = 422, retryable: bool = False):
        super().__init__(message)
        self.code = code
        self.message = message
        self.status = status
        self.retryable = retryable

    def payload(self) -> dict[str, Any]:
        return {"code": self.code, "message": self.message, "retryable": self.retryable}


def require_object(value: Any, name: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise MathWorkerError("math_protocol_invalid", f"{name} must be an object", 400)
    return value


def require_list(value: Any, name: str) -> list[Any]:
    if not isinstance(value, list):
        raise MathWorkerError("math_protocol_invalid", f"{name} must be an array", 400)
    return value


def validate_symbol_name(name: Any) -> str:
    if (
        not isinstance(name, str)
        or not name.isidentifier()
        or name.startswith("_")
        or "__" in name
        or len(name.encode("utf-8")) > 64
    ):
        raise MathWorkerError("math_symbol_invalid", "invalid symbol name", 400)
    return name


def validate_ast(node: Any, depth: int = 1, counter: list[int] | None = None) -> None:
    if counter is None:
        counter = [0]
    counter[0] += 1
    if counter[0] > MAX_AST_NODES:
        raise MathWorkerError("math_expression_too_complex", "AST node limit exceeded", 413)
    if depth > MAX_AST_DEPTH:
        raise MathWorkerError("math_expression_too_deep", "AST depth limit exceeded", 413)
    node = require_object(node, "expression")
    kind = node.get("kind")
    if kind == "integer":
        if not isinstance(node.get("value"), str) or not INTEGER.fullmatch(node["value"]):
            raise MathWorkerError("math_number_invalid", "invalid integer literal", 400)
        return
    if kind == "decimal":
        if not isinstance(node.get("value"), str) or not NUMBER.fullmatch(node["value"]):
            raise MathWorkerError("math_number_invalid", "invalid decimal literal", 400)
        return
    if kind == "symbol":
        validate_symbol_name(node.get("name"))
        return
    if kind == "constant":
        if node.get("value") not in CONSTANTS:
            raise MathWorkerError("math_constant_invalid", "unsupported constant", 400)
        return
    if kind == "negate":
        validate_ast(node.get("value"), depth + 1, counter)
        return
    if kind == "add":
        children = require_list(node.get("terms"), "terms")
        if len(children) < 2:
            raise MathWorkerError("math_protocol_invalid", "add requires two terms", 400)
    elif kind == "multiply":
        children = require_list(node.get("factors"), "factors")
        if len(children) < 2:
            raise MathWorkerError("math_protocol_invalid", "multiply requires two factors", 400)
    elif kind == "divide":
        children = [node.get("numerator"), node.get("denominator")]
    elif kind == "power":
        children = [node.get("base"), node.get("exponent")]
    elif kind == "function":
        if node.get("function") not in FUNCTIONS:
            raise MathWorkerError("math_function_unsupported", "unsupported function", 400)
        children = require_list(node.get("arguments"), "arguments")
        if not children:
            raise MathWorkerError("math_function_arity_invalid", "function has no arguments", 400)
    else:
        raise MathWorkerError("math_ast_kind_invalid", f"unsupported AST kind: {kind}", 400)
    for child in children:
        validate_ast(child, depth + 1, counter)


def symbol_table(assumptions: Any) -> dict[str, sp.Symbol]:
    assumptions = require_object(assumptions, "assumptions")
    if len(assumptions) > 64:
        raise MathWorkerError("math_too_many_variables", "assumption variable limit exceeded", 413)
    symbols: dict[str, sp.Symbol] = {}
    for raw_name, raw_values in assumptions.items():
        name = validate_symbol_name(raw_name)
        values = require_list(raw_values, f"assumptions for {name}")
        kwargs: dict[str, bool] = {}
        for value in values:
            key = ASSUMPTIONS.get(value)
            if key is None:
                raise MathWorkerError("math_assumption_invalid", f"unsupported assumption: {value}", 400)
            kwargs[key] = True
        try:
            symbols[name] = sp.Symbol(name, **kwargs)
        except Exception as error:
            raise MathWorkerError("math_assumptions_conflict", str(error), 400) from error
    return symbols


def decimal_rational(value: str) -> sp.Rational:
    try:
        decimal = Decimal(value)
    except InvalidOperation as error:
        raise MathWorkerError("math_number_invalid", "invalid decimal literal", 400) from error
    numerator, denominator = decimal.as_integer_ratio()
    return sp.Rational(numerator, denominator)


def from_ast(node: Any, symbols: dict[str, sp.Symbol]) -> sp.Expr:
    validate_ast(node)
    return from_validated_ast(node, symbols)


def from_validated_ast(node: dict[str, Any], symbols: dict[str, sp.Symbol]) -> sp.Expr:
    kind = node["kind"]
    if kind == "integer":
        return sp.Integer(node["value"])
    if kind == "decimal":
        return decimal_rational(node["value"])
    if kind == "symbol":
        name = node["name"]
        if name not in symbols:
            symbols[name] = sp.Symbol(name)
        return symbols[name]
    if kind == "constant":
        return CONSTANTS[node["value"]]
    if kind == "negate":
        return -from_validated_ast(node["value"], symbols)
    if kind == "add":
        return sp.Add(*(from_validated_ast(term, symbols) for term in node["terms"]))
    if kind == "multiply":
        return sp.Mul(*(from_validated_ast(factor, symbols) for factor in node["factors"]))
    if kind == "divide":
        return from_validated_ast(node["numerator"], symbols) / from_validated_ast(
            node["denominator"], symbols
        )
    if kind == "power":
        return from_validated_ast(node["base"], symbols) ** from_validated_ast(
            node["exponent"], symbols
        )
    function = FUNCTIONS[node["function"]]
    return function(*(from_validated_ast(argument, symbols) for argument in node["arguments"]))


def equation_from_ast(value: Any, symbols: dict[str, sp.Symbol]) -> sp.Equality:
    value = require_object(value, "equation")
    return sp.Eq(
        from_ast(value.get("left"), symbols),
        from_ast(value.get("right"), symbols),
        evaluate=False,
    )


def math_value(value: Any, precision: int | None = None) -> dict[str, Any]:
    if not isinstance(value, sp.Basic):
        if isinstance(value, int):
            value = sp.Integer(value)
        else:
            raise MathWorkerError(
                "math_backend_value_invalid", "symbolic backend returned an unsupported value", 500
            )
    approximate = None
    if precision is not None and not value.free_symbols:
        try:
            approximate = sp.sstr(sp.N(value, precision))
        except Exception:
            approximate = None
    return {
        "text": sp.sstr(value).replace("**", "^"),
        "latex": sp.latex(value),
        "approximate": approximate,
        "freeSymbols": sorted(str(symbol) for symbol in value.free_symbols),
    }


def value_result(value: Any, precision: int | None = None) -> dict[str, Any]:
    return {"kind": "value", "value": math_value(value, precision)}


def domain_value(domain: str) -> sp.Set:
    if domain == "complex":
        return sp.S.Complexes
    if domain == "real":
        return sp.S.Reals
    if domain == "integer":
        return sp.S.Integers
    raise MathWorkerError("math_domain_invalid", f"unsupported solve domain: {domain}", 400)


def simplify_expression(expression: sp.Expr, method: str) -> sp.Expr:
    functions = {
        "smart": sp.simplify,
        "factor": sp.factor,
        "expand": sp.expand,
        "cancel": sp.cancel,
        "together": sp.together,
        "trigonometric": sp.trigsimp,
    }
    function = functions.get(method)
    if function is None:
        raise MathWorkerError("math_simplification_invalid", "unsupported simplification method", 400)
    return function(expression)


def solve_operation(
    operation: dict[str, Any], symbols: dict[str, sp.Symbol]
) -> tuple[dict[str, Any], list[str]]:
    raw_equations = require_list(operation.get("equations"), "equations")
    variables = [symbols.setdefault(validate_symbol_name(name), sp.Symbol(name)) for name in operation.get("variables", [])]
    equations = [equation_from_ast(equation, symbols) for equation in raw_equations]
    domain = domain_value(operation.get("domain", "complex"))
    warnings: list[str] = []

    if len(equations) == 1 and len(variables) == 1:
        solution_set = sp.solveset(equations[0], variables[0], domain=domain)
        if isinstance(solution_set, sp.FiniteSet):
            solutions = [
                {"assignments": {str(variables[0]): math_value(value)}}
                for value in sorted(solution_set, key=sp.default_sort_key)
            ]
            return {"kind": "solutions", "solutions": solutions, "solutionSet": None}, warnings
        return {
            "kind": "solutions",
            "solutions": [],
            "solutionSet": math_value(solution_set),
        }, warnings

    if domain != sp.S.Complexes:
        warnings.append(
            "domain filtering for multivariable systems is conservative; verify returned assignments against the requested domain"
        )
    solved = sp.solve(equations, variables, dict=True)
    if isinstance(solved, dict):
        solved = [solved]
    solutions = []
    for assignment in solved:
        if not isinstance(assignment, dict):
            continue
        solutions.append(
            {
                "assignments": {
                    str(variable): math_value(value)
                    for variable, value in sorted(assignment.items(), key=lambda item: str(item[0]))
                }
            }
        )
    return {"kind": "solutions", "solutions": solutions, "solutionSet": None}, warnings


def verification_result(
    difference: sp.Expr,
    *,
    seed_offset: int = 0,
) -> dict[str, Any]:
    try:
        difference = sp.trigsimp(sp.cancel(sp.together(sp.simplify(difference))))
    except Exception:
        difference = sp.simplify(difference)
    if difference == 0 or difference.is_zero is True:
        return {
            "kind": "verification",
            "status": "proven",
            "valid": True,
            "difference": math_value(difference),
            "counterexample": {},
        }
    if not difference.free_symbols and difference.is_zero is False:
        return {
            "kind": "verification",
            "status": "disproven",
            "valid": False,
            "difference": math_value(difference),
            "counterexample": {},
        }

    free_symbols = sorted(difference.free_symbols, key=str)
    sample_sets = {symbol: samples_for(symbol) for symbol in free_symbols}
    sample_count = max((len(values) for values in sample_sets.values()), default=0)
    for sample_index in range(sample_count):
        assignment = {
            symbol: sample_sets[symbol][
                (sample_index + index + seed_offset) % len(sample_sets[symbol])
            ]
            for index, symbol in enumerate(free_symbols)
        }
        try:
            observed = sp.simplify(difference.subs(assignment))
            numeric = complex(sp.N(observed, 30))
        except (TypeError, ValueError, ZeroDivisionError, OverflowError):
            continue
        if math.isfinite(numeric.real) and math.isfinite(numeric.imag) and abs(numeric) > 1e-20:
            return {
                "kind": "verification",
                "status": "disproven",
                "valid": False,
                "difference": math_value(difference),
                "counterexample": {str(symbol): sp.sstr(value) for symbol, value in assignment.items()},
            }
    return {
        "kind": "verification",
        "status": "inconclusive",
        "valid": None,
        "difference": math_value(difference),
        "counterexample": {},
    }


def samples_for(symbol: sp.Symbol) -> list[sp.Expr]:
    if symbol.is_positive is True:
        return [sp.Rational(1, 2), sp.Integer(1), sp.Integer(2), sp.Integer(3)]
    if symbol.is_nonnegative is True:
        return [sp.Integer(0), sp.Rational(1, 2), sp.Integer(1), sp.Integer(2)]
    if symbol.is_integer is True:
        return [sp.Integer(-3), sp.Integer(-2), sp.Integer(-1), sp.Integer(1), sp.Integer(2), sp.Integer(3)]
    return [
        sp.Integer(-3),
        sp.Integer(-2),
        sp.Integer(-1),
        sp.Rational(1, 2),
        sp.Integer(1),
        sp.Integer(2),
        sp.Integer(3),
    ]


def domain_counterexample(left: sp.Expr, right: sp.Expr) -> dict[str, str] | None:
    symbols = sorted(left.free_symbols | right.free_symbols, key=str)
    if len(symbols) != 1:
        return None
    symbol = symbols[0]
    try:
        left_singularities = sp.singularities(left, symbol)
        right_singularities = sp.singularities(right, symbol)
        mismatch = sp.SymmetricDifference(left_singularities, right_singularities)
        if isinstance(mismatch, sp.FiniteSet) and mismatch:
            value = sorted(mismatch, key=sp.default_sort_key)[0]
            return {str(symbol): sp.sstr(value).replace("**", "^")}
    except (NotImplementedError, ValueError, TypeError):
        return None
    return None


def verify_equivalence(left: sp.Expr, right: sp.Expr, seed_offset: int = 0) -> dict[str, Any]:
    counterexample = domain_counterexample(left, right)
    if counterexample is not None:
        return {
            "kind": "verification",
            "status": "disproven",
            "valid": False,
            "difference": math_value(left - right),
            "counterexample": counterexample,
        }
    return verification_result(left - right, seed_offset=seed_offset)


def verify_operation(operation: dict[str, Any], symbols: dict[str, sp.Symbol]) -> dict[str, Any]:
    assertion = require_object(operation.get("assertion"), "assertion")
    kind = assertion.get("kind")
    if kind == "equivalent":
        left = from_ast(assertion.get("left"), symbols)
        right = from_ast(assertion.get("right"), symbols)
        return verify_equivalence(left, right)
    if kind == "derivative":
        variable_name = validate_symbol_name(assertion.get("variable"))
        variable = symbols.setdefault(variable_name, sp.Symbol(variable_name))
        expected = sp.diff(from_ast(assertion.get("expression"), symbols), variable, int(assertion.get("order", 1)))
        candidate = from_ast(assertion.get("candidate"), symbols)
        return verify_equivalence(expected, candidate, seed_offset=1)
    if kind == "antiderivative":
        variable_name = validate_symbol_name(assertion.get("variable"))
        variable = symbols.setdefault(variable_name, sp.Symbol(variable_name))
        integrand = from_ast(assertion.get("expression"), symbols)
        candidate = from_ast(assertion.get("candidate"), symbols)
        return verify_equivalence(sp.diff(candidate, variable), integrand, seed_offset=2)
    if kind == "satisfies":
        values = require_object(assertion.get("values"), "values")
        substitutions = {
            symbols.setdefault(validate_symbol_name(name), sp.Symbol(name)): from_ast(value, symbols)
            for name, value in values.items()
        }
        unknown = False
        for equation in require_list(assertion.get("equations"), "equations"):
            equation = equation_from_ast(equation, symbols)
            result = verification_result((equation.lhs - equation.rhs).subs(substitutions))
            if result["status"] == "disproven":
                return result
            if result["status"] == "inconclusive":
                unknown = True
        return {
            "kind": "verification",
            "status": "inconclusive" if unknown else "proven",
            "valid": None if unknown else True,
            "difference": None,
            "counterexample": {},
        }
    raise MathWorkerError("math_verification_invalid", "unsupported verification assertion", 400)


def execute_request(request: Any) -> dict[str, Any]:
    started = time.monotonic()
    request = require_object(request, "request")
    symbols = symbol_table(request.get("assumptions", {}))
    operation = require_object(request.get("operation"), "operation")
    name = operation.get("operation")
    warnings: list[str] = []

    if name == "calculate":
        expression = from_ast(operation.get("expression"), symbols)
        values = require_object(operation.get("variables"), "variables")
        substitutions = {
            symbols.setdefault(validate_symbol_name(variable), sp.Symbol(variable)): from_ast(value, symbols)
            for variable, value in values.items()
        }
        expression = sp.simplify(expression.subs(substitutions))
        if expression.free_symbols:
            names = ", ".join(sorted(str(symbol) for symbol in expression.free_symbols))
            raise MathWorkerError("math_calculation_variables_missing", f"values missing for: {names}", 422)
        result = value_result(expression, int(operation.get("precision", 50)))
    elif name == "simplify":
        expression = from_ast(operation.get("expression"), symbols)
        result = value_result(simplify_expression(expression, operation.get("method", "smart")))
    elif name == "differentiate":
        expression = from_ast(operation.get("expression"), symbols)
        variable_name = validate_symbol_name(operation.get("variable"))
        variable = symbols.setdefault(variable_name, sp.Symbol(variable_name))
        result = value_result(sp.diff(expression, variable, int(operation.get("order", 1))))
    elif name == "integrate":
        expression = from_ast(operation.get("expression"), symbols)
        variable_name = validate_symbol_name(operation.get("variable"))
        variable = symbols.setdefault(variable_name, sp.Symbol(variable_name))
        bounds = operation.get("bounds")
        if bounds is None:
            integrated = sp.integrate(expression, variable)
        else:
            bounds = require_object(bounds, "bounds")
            integrated = sp.integrate(
                expression,
                (variable, from_ast(bounds.get("lower"), symbols), from_ast(bounds.get("upper"), symbols)),
            )
        result = value_result(integrated)
    elif name == "limit":
        expression = from_ast(operation.get("expression"), symbols)
        variable_name = validate_symbol_name(operation.get("variable"))
        variable = symbols.setdefault(variable_name, sp.Symbol(variable_name))
        point = from_ast(operation.get("point"), symbols)
        direction = operation.get("direction", "both")
        if direction == "left":
            limited = sp.limit(expression, variable, point, dir="-")
        elif direction == "right":
            limited = sp.limit(expression, variable, point, dir="+")
        elif direction == "both":
            left = sp.limit(expression, variable, point, dir="-")
            right = sp.limit(expression, variable, point, dir="+")
            if left != right and sp.simplify(left - right) != 0:
                raise MathWorkerError(
                    "math_limit_does_not_exist",
                    f"left limit is {sp.sstr(left)} while right limit is {sp.sstr(right)}",
                    422,
                )
            limited = left
        else:
            raise MathWorkerError("math_limit_direction_invalid", "unsupported limit direction", 400)
        result = value_result(limited)
    elif name == "solve":
        result, warnings = solve_operation(operation, symbols)
    elif name == "verify":
        result = verify_operation(operation, symbols)
    else:
        raise MathWorkerError("math_operation_unsupported", f"unsupported operation: {name}", 400)

    response = {
        "result": result,
        "warnings": warnings,
        "backend": f"sympy-{sp.__version__}",
        "durationMs": max(0, int((time.monotonic() - started) * 1000)),
    }
    encoded = json.dumps(response, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    if len(encoded) > MAX_RESPONSE_BYTES:
        raise MathWorkerError("math_response_too_large", "symbolic result exceeds output limit", 413)
    return response


def child_entry(request: dict[str, Any], connection: Any) -> None:
    try:
        try:
            import resource

            cpu_limit = max(1, OPERATION_TIMEOUT_SECONDS)
            resource.setrlimit(resource.RLIMIT_CPU, (cpu_limit, cpu_limit + 1))
        except (ImportError, OSError, ValueError):
            pass
        connection.send({"ok": True, "response": execute_request(request)})
    except MathWorkerError as error:
        connection.send({"ok": False, "error": error.payload(), "status": error.status})
    except Exception as error:
        connection.send(
            {
                "ok": False,
                "error": {
                    "code": "math_backend_failed",
                    "message": f"symbolic backend failed: {type(error).__name__}",
                    "retryable": False,
                },
                "status": 422,
            }
        )
    finally:
        connection.close()


def run_isolated(request: dict[str, Any]) -> tuple[int, dict[str, Any]]:
    context = multiprocessing.get_context("spawn")
    parent, child = context.Pipe(duplex=False)
    process = context.Process(target=child_entry, args=(request, child), daemon=True)
    process.start()
    child.close()
    try:
        if not parent.poll(OPERATION_TIMEOUT_SECONDS):
            process.terminate()
            process.join(timeout=1)
            if process.is_alive():
                process.kill()
                process.join(timeout=1)
            return HTTPStatus.REQUEST_TIMEOUT, {
                "code": "math_operation_timeout",
                "message": f"math operation exceeded {OPERATION_TIMEOUT_SECONDS} seconds",
                "retryable": True,
            }
        message = parent.recv()
        process.join(timeout=1)
        if message.get("ok"):
            return HTTPStatus.OK, message["response"]
        return int(message.get("status", 422)), message["error"]
    except EOFError:
        return HTTPStatus.UNPROCESSABLE_ENTITY, {
            "code": "math_backend_terminated",
            "message": "math backend terminated before returning a result",
            "retryable": False,
        }
    finally:
        parent.close()
        if process.is_alive():
            process.terminate()
            process.join(timeout=1)


class MathServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, address: tuple[str, int]):
        super().__init__(address, Handler)
        self.capacity = threading.BoundedSemaphore(MAX_CONCURRENCY)


class Handler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if self.path != "/health":
            self.send_json(HTTPStatus.NOT_FOUND, {"code": "not_found", "message": "not found"})
            return
        self.send_json(
            HTTPStatus.OK,
            {
                "status": "ok",
                "service": "safe-math",
                "backend": f"sympy-{sp.__version__}",
                "maxConcurrency": MAX_CONCURRENCY,
                "operationTimeoutSeconds": OPERATION_TIMEOUT_SECONDS,
            },
        )

    def do_POST(self) -> None:
        server = cast(MathServer, self.server)
        if self.path != "/v1/math":
            self.send_json(HTTPStatus.NOT_FOUND, {"code": "not_found", "message": "not found"})
            return
        content_type = self.headers.get("Content-Type", "").split(";", 1)[0].strip().lower()
        if content_type != "application/json":
            self.send_json(
                HTTPStatus.UNSUPPORTED_MEDIA_TYPE,
                {"code": "math_content_type_invalid", "message": "Content-Type must be application/json"},
            )
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            length = -1
        if length <= 0 or length > MAX_REQUEST_BYTES:
            self.send_json(
                HTTPStatus.REQUEST_ENTITY_TOO_LARGE,
                {"code": "math_request_too_large", "message": "invalid math request size"},
            )
            return
        try:
            request = json.loads(self.rfile.read(length))
        except (json.JSONDecodeError, UnicodeDecodeError):
            self.send_json(
                HTTPStatus.BAD_REQUEST,
                {"code": "math_json_invalid", "message": "request body is not valid JSON"},
            )
            return
        if not server.capacity.acquire(blocking=False):
            self.send_json(
                HTTPStatus.TOO_MANY_REQUESTS,
                {"code": "math_worker_busy", "message": "math worker is at capacity", "retryable": True},
            )
            return
        try:
            status, response = run_isolated(request)
            self.send_json(status, response)
        finally:
            server.capacity.release()

    def send_json(self, status: int, value: dict[str, Any]) -> None:
        body = json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *arguments: Any) -> None:
        # Request paths and status are useful; expressions and bodies never go
        # to logs because they may contain private coursework or user data.
        print(f"math-worker {self.client_address[0]} {format % arguments}", flush=True)


def main() -> None:
    server = MathServer(("0.0.0.0", PORT))
    print(
        f"safe-math listening on :{PORT} with sympy {sp.__version__}, concurrency {MAX_CONCURRENCY}",
        flush=True,
    )
    try:
        server.serve_forever(poll_interval=0.5)
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
