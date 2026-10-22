import unittest

from worker import MathWorkerError, execute_request


def integer(value: int):
    return {"kind": "integer", "value": str(value)}


def symbol(name: str):
    return {"kind": "symbol", "name": name}


def power(base, exponent):
    return {"kind": "power", "base": base, "exponent": exponent}


def add(*terms):
    return {"kind": "add", "terms": list(terms)}


class WorkerTests(unittest.TestCase):
    def execute(self, operation):
        return execute_request({"operation": operation, "assumptions": {}})

    def test_differentiates_without_parsing_raw_python(self):
        response = self.execute(
            {
                "operation": "differentiate",
                "expression": add(power(symbol("x"), integer(3)), symbol("x")),
                "variable": "x",
                "order": 1,
            }
        )
        self.assertEqual(response["result"]["value"]["text"], "3*x^2 + 1")

    def test_solves_a_real_quadratic(self):
        response = self.execute(
            {
                "operation": "solve",
                "equations": [
                    {
                        "left": add(power(symbol("x"), integer(2)), {"kind": "negate", "value": integer(4)}),
                        "right": integer(0),
                    }
                ],
                "variables": ["x"],
                "domain": "real",
            }
        )
        values = [solution["assignments"]["x"]["text"] for solution in response["result"]["solutions"]]
        self.assertEqual(values, ["-2", "2"])

    def test_proves_an_identity_and_rejects_a_false_derivative(self):
        identity = self.execute(
            {
                "operation": "verify",
                "assertion": {
                    "kind": "equivalent",
                    "left": power({"kind": "function", "function": "sin", "arguments": [symbol("x")]}, integer(2)),
                    "right": add(
                        integer(1),
                        {"kind": "negate", "value": power({"kind": "function", "function": "cos", "arguments": [symbol("x")]}, integer(2))},
                    ),
                },
            }
        )
        self.assertEqual(identity["result"]["status"], "proven")

        derivative = self.execute(
            {
                "operation": "verify",
                "assertion": {
                    "kind": "derivative",
                    "expression": power(symbol("x"), integer(2)),
                    "variable": "x",
                    "candidate": symbol("x"),
                    "order": 1,
                },
            }
        )
        self.assertEqual(derivative["result"]["status"], "disproven")

    def test_equivalence_preserves_excluded_domain_points(self):
        left = {
            "kind": "divide",
            "numerator": add(power(symbol("x"), integer(2)), {"kind": "negate", "value": integer(1)}),
            "denominator": add(symbol("x"), {"kind": "negate", "value": integer(1)}),
        }
        right = add(symbol("x"), integer(1))
        response = self.execute(
            {
                "operation": "verify",
                "assertion": {"kind": "equivalent", "left": left, "right": right},
            }
        )
        self.assertEqual(response["result"]["status"], "disproven")
        self.assertEqual(response["result"]["counterexample"], {"x": "1"})

    def test_rejects_unknown_ast_kinds(self):
        with self.assertRaises(MathWorkerError):
            self.execute({"operation": "simplify", "expression": {"kind": "python", "value": "__import__('os')"}, "method": "smart"})


if __name__ == "__main__":
    unittest.main()
