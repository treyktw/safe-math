"""Structured mathematics, written as LaTeX.

The scene protocol carries an expression tree rather than a LaTeX string or a
snippet of Python. That is a deliberate boundary: nothing a model writes reaches
this container as code, so a render cannot become arbitrary execution. The cost
is this file -- the tree has to be turned into something Manim can typeset.
"""

from __future__ import annotations

from typing import Any

# Where a symbol has a conventional name, use it. `\\pi` reads as pi; `pi`
# reads as a variable called p times i.
CONSTANTS = {
    "pi": r"\pi",
    "e": "e",
    "i": "i",
    "infinity": r"\infty",
    "tau": r"\tau",
}

# Functions LaTeX already names. Anything absent falls through to \operatorname,
# which is how an unknown function should look rather than italic juxtaposition.
FUNCTIONS = {
    "sin": r"\sin",
    "cos": r"\cos",
    "tan": r"\tan",
    "asin": r"\arcsin",
    "acos": r"\arccos",
    "atan": r"\arctan",
    "sinh": r"\sinh",
    "cosh": r"\cosh",
    "tanh": r"\tanh",
    "log": r"\log",
    "ln": r"\ln",
    "exp": r"\exp",
    "min": r"\min",
    "max": r"\max",
}

# Binding strength, so parentheses appear exactly where they change meaning.
# Without this, `a + b` inside a power renders as `a + b^2`, which is a
# different expression that looks deliberate.
PRECEDENCE = {
    "add": 1,
    "multiply": 2,
    "divide": 2,
    "negate": 2,
    "power": 3,
    "atom": 4,
}


def to_latex(node: Any, parent: int = 0) -> str:
    """Render one expression node."""
    if not isinstance(node, dict):
        return _escape(str(node))

    kind = node.get("kind")

    if kind in {"integer", "decimal"}:
        return _escape(str(node.get("value", "")))

    if kind == "symbol":
        return _symbol(str(node.get("name", "")))

    if kind == "constant":
        value = str(node.get("value", ""))
        return CONSTANTS.get(value, _escape(value))

    if kind == "negate":
        inner = to_latex(node.get("value"), PRECEDENCE["negate"])
        return _wrap(f"-{inner}", PRECEDENCE["negate"], parent)

    if kind == "add":
        terms = node.get("terms") or []
        if not terms:
            return "0"
        out = to_latex(terms[0], PRECEDENCE["add"])
        for term in terms[1:]:
            rendered = to_latex(term, PRECEDENCE["add"])
            # A negative term reads as subtraction. "a + -b" is correct and
            # nobody writes it.
            out += f" - {rendered[1:]}" if rendered.startswith("-") else f" + {rendered}"
        return _wrap(out, PRECEDENCE["add"], parent)

    if kind == "multiply":
        factors = node.get("factors") or []
        if not factors:
            return "1"
        rendered = [to_latex(factor, PRECEDENCE["multiply"]) for factor in factors]
        return _wrap(" ".join(rendered), PRECEDENCE["multiply"], parent)

    if kind == "divide":
        # \frac takes its own arguments, so neither side needs parentheses --
        # the fraction bar already groups them.
        numerator = to_latex(node.get("numerator"), 0)
        denominator = to_latex(node.get("denominator"), 0)
        return rf"\frac{{{numerator}}}{{{denominator}}}"

    if kind == "power":
        base = to_latex(node.get("base"), PRECEDENCE["power"] + 1)
        exponent = to_latex(node.get("exponent"), 0)
        return _wrap(f"{base}^{{{exponent}}}", PRECEDENCE["power"], parent)

    if kind == "function":
        name = str(node.get("function", ""))
        arguments = [to_latex(argument, 0) for argument in node.get("arguments") or []]
        joined = ", ".join(arguments)

        # A square root is a radical, not sqrt(x) in a serif font.
        if name == "sqrt" and len(arguments) == 1:
            return rf"\sqrt{{{arguments[0]}}}"
        if name == "abs" and len(arguments) == 1:
            return rf"\left|{arguments[0]}\right|"

        command = FUNCTIONS.get(name, rf"\operatorname{{{_escape(name)}}}")
        return rf"{command}\left({joined}\right)"

    # An unrecognised node renders as nothing rather than as the word "None".
    return ""


def equation_to_latex(equation: Any) -> str:
    """Both sides and the relation between them."""
    if not isinstance(equation, dict):
        return _escape(str(equation))

    left = to_latex(equation.get("left"), 0)
    right = to_latex(equation.get("right"), 0)
    relation = {
        "equal": "=",
        "not_equal": r"\neq",
        "less": "<",
        "less_or_equal": r"\leq",
        "greater": ">",
        "greater_or_equal": r"\geq",
    }.get(str(equation.get("relation", "equal")), "=")

    return f"{left} {relation} {right}"


def _symbol(name: str) -> str:
    """Greek letters written as themselves."""
    greek = {
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta",
        "iota", "kappa", "lambda", "mu", "nu", "xi", "rho", "sigma", "tau",
        "phi", "chi", "psi", "omega",
    }
    lowered = name.lower()
    if lowered in greek:
        return rf"\{lowered}" if name == lowered else rf"\{lowered.capitalize()}"

    # A multi-character name is a word, not a product of letters.
    return name if len(name) == 1 else rf"\mathrm{{{_escape(name)}}}"


def _wrap(rendered: str, own: int, parent: int) -> str:
    return rf"\left({rendered}\right)" if own < parent else rendered


def _escape(text: str) -> str:
    """Characters that would otherwise be markup."""
    for character, replacement in (
        ("\\", r"\backslash "),
        ("{", r"\{"),
        ("}", r"\}"),
        ("_", r"\_"),
        ("^", r"\^{}"),
        ("&", r"\&"),
        ("%", r"\%"),
        ("$", r"\$"),
        ("#", r"\#"),
    ):
        text = text.replace(character, replacement)
    return text
