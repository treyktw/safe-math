"""What the converter has to get right.

Parenthesisation is the whole risk here: a missing pair does not fail, it
renders a different equation that looks deliberate.
"""

import unittest

from latex import equation_to_latex, to_latex


def integer(value):
    return {"kind": "integer", "value": str(value)}


def symbol(name):
    return {"kind": "symbol", "name": name}


class LatexTests(unittest.TestCase):
    def test_atoms(self):
        self.assertEqual(to_latex(integer(3)), "3")
        self.assertEqual(to_latex(symbol("x")), "x")
        self.assertEqual(to_latex({"kind": "constant", "value": "pi"}), r"\pi")


    def test_a_sum_inside_a_power_keeps_its_parentheses(self):
        # Without them this is x + y^2, which is a different expression.
        node = {
            "kind": "power",
            "base": {"kind": "add", "terms": [symbol("x"), symbol("y")]},
            "exponent": integer(2),
        }
        self.assertEqual(to_latex(node), r"\left(x + y\right)^{2}")


    def test_a_sum_inside_a_product_keeps_its_parentheses(self):
        node = {
            "kind": "multiply",
            "factors": [integer(2), {"kind": "add", "terms": [symbol("x"), integer(1)]}],
        }
        self.assertEqual(to_latex(node), r"2 \left(x + 1\right)")


    def test_a_fraction_needs_no_parentheses_around_its_parts(self):
        # The bar groups them; adding parentheses would be noise.
        node = {
            "kind": "divide",
            "numerator": {"kind": "add", "terms": [symbol("x"), integer(1)]},
            "denominator": integer(2),
        }
        self.assertEqual(to_latex(node), r"\frac{x + 1}{2}")


    def test_a_negative_term_reads_as_subtraction(self):
        # "a + -b" is correct and nobody writes it.
        node = {
            "kind": "add",
            "terms": [symbol("a"), {"kind": "negate", "value": symbol("b")}],
        }
        self.assertEqual(to_latex(node), "a - b")


    def test_known_functions_use_their_own_command(self):
        node = {"kind": "function", "function": "sin", "arguments": [symbol("x")]}
        self.assertEqual(to_latex(node), r"\sin\left(x\right)")


    def test_an_unknown_function_is_upright_rather_than_italic_letters(self):
        node = {"kind": "function", "function": "erf", "arguments": [symbol("x")]}
        self.assertEqual(to_latex(node), r"\operatorname{erf}\left(x\right)")


    def test_a_square_root_is_a_radical(self):
        node = {"kind": "function", "function": "sqrt", "arguments": [symbol("x")]}
        self.assertEqual(to_latex(node), r"\sqrt{x}")


    def test_greek_letters_are_written_as_themselves(self):
        self.assertEqual(to_latex(symbol("theta")), r"\theta")
        self.assertEqual(to_latex(symbol("x")), "x")


    def test_a_word_is_not_a_product_of_letters(self):
        self.assertEqual(to_latex(symbol("rate")), r"\mathrm{rate}")


    def test_an_equation_carries_its_relation(self):
        equation = {
            "left": {"kind": "power", "base": symbol("x"), "exponent": integer(2)},
            "right": integer(4),
            "relation": "equal",
        }
        self.assertEqual(equation_to_latex(equation), "x^{2} = 4")


    def test_an_unknown_node_renders_as_nothing_rather_than_the_word_none(self):
        self.assertEqual(to_latex({"kind": "future_thing"}), "")


if __name__ == "__main__":
    unittest.main()
