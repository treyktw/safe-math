"""What the scene builder has to get right without a renderer present.

Manim is stubbed here. These tests are about the two things that are wrong in
a way no exception reports: reading the wire format the Rust protocol actually
emits, and evaluating an expression tree without handing the caller `eval`.
"""

import math
import sys
import types
import unittest


def _stub_manim() -> None:
    """A stand-in for Manim, so the wire handling can be tested anywhere.

    The renderer is a 700 MB image with a TeX distribution in it. Requiring it
    to check that `object_id` is read under the right key would mean these
    tests only ever ran in CI, which is the same as not having them.
    """
    if "manim" in sys.modules:
        return
    module = types.ModuleType("manim")
    for name in (
        "Arrow", "Axes", "Brace", "Circle", "Create", "Dot", "FadeIn", "FadeOut",
        "Line", "MathTex", "Rectangle", "Scene", "Square", "Text", "Transform",
        "Triangle", "VGroup", "Write",
    ):
        setattr(module, name, type(name, (), {}))
    module.linear = lambda t: t
    module.smooth = lambda t: t
    module.rate_functions = types.SimpleNamespace(
        ease_in_sine=lambda t: t, ease_out_sine=lambda t: t, ease_in_out_sine=lambda t: t
    )
    sys.modules["manim"] = module


_stub_manim()

from scene import _cues, duration_of, evaluate, field  # noqa: E402


def integer(value):
    return {"kind": "integer", "value": str(value)}


def symbol(name):
    return {"kind": "symbol", "name": name}


class FieldTests(unittest.TestCase):
    def test_reads_the_snake_case_serde_emits_for_enum_variant_fields(self):
        # serde's rename_all on an enum renames variants, not the fields inside
        # them. AnimationAction therefore ships `object_id`, while the structs
        # around it ship camelCase. Reading only one spelling loses every
        # animation in the scene and renders a still frame that looks fine.
        self.assertEqual(field({"object_id": "a"}, "object_id", "objectId"), "a")
        self.assertEqual(field({"objectId": "a"}, "object_id", "objectId"), "a")

    def test_a_null_is_not_a_value(self):
        self.assertEqual(field({"axes_id": None, "axesId": "b"}, "axes_id", "axesId"), "b")

    def test_missing_falls_through_to_the_default(self):
        self.assertEqual(field({}, "width", default=6.0), 6.0)
        self.assertIsNone(field("not an object", "width"))


class EvaluateTests(unittest.TestCase):
    def test_evaluates_a_polynomial(self):
        # 2x^2 + 1 at x = 3
        node = {
            "kind": "add",
            "terms": [
                {
                    "kind": "multiply",
                    "factors": [
                        integer(2),
                        {"kind": "power", "base": symbol("x"), "exponent": integer(2)},
                    ],
                },
                integer(1),
            ],
        }
        self.assertEqual(evaluate(node, {"x": 3.0}), 19.0)

    def test_a_constant_is_a_number_not_a_lookup_of_whatever_was_sent(self):
        self.assertAlmostEqual(evaluate({"kind": "constant", "value": "pi"}, {}), math.pi)

    def test_imaginary_and_infinite_constants_are_not_points_on_a_curve(self):
        # Both are in the AST and neither has a y value. Returning 0.0 would
        # draw a line through the axis and call it the function.
        for name in ("i", "infinity"):
            with self.assertRaises(ValueError):
                evaluate({"kind": "constant", "value": name}, {})

    def test_an_unbound_symbol_raises_rather_than_reading_as_zero(self):
        with self.assertRaises(ValueError):
            evaluate(symbol("y"), {"x": 1.0})

    def test_division_by_zero_is_a_gap_not_a_value(self):
        with self.assertRaises(ZeroDivisionError):
            evaluate({"kind": "divide", "numerator": integer(1), "denominator": integer(0)}, {})

    def test_only_named_functions_are_reachable(self):
        with self.assertRaises(ValueError):
            evaluate({"kind": "function", "function": "system", "arguments": []}, {})

    def test_a_node_kind_it_does_not_know_is_refused(self):
        with self.assertRaises(ValueError):
            evaluate({"kind": "python", "value": "__import__('os')"}, {})


class TimelineTests(unittest.TestCase):
    def test_duration_comes_from_the_last_step_to_end_not_the_last_listed(self):
        timeline = [
            {"startSeconds": 0.0, "durationSeconds": 10.0},
            {"startSeconds": 2.0, "durationSeconds": 1.0},
        ]
        self.assertEqual(duration_of(timeline), 10.0)

    def test_steps_sharing_a_start_are_one_cue(self):
        # Playing them in sequence would stretch the scene past its own
        # timeline and desynchronise everything after them.
        timeline = [
            {"startSeconds": 1.0, "durationSeconds": 1.0, "action": {"action": "wait"}},
            {"startSeconds": 0.0, "durationSeconds": 1.0, "action": {"action": "wait"}},
            {"startSeconds": 1.0, "durationSeconds": 2.0, "action": {"action": "wait"}},
        ]
        cues = _cues(timeline)
        self.assertEqual([start for start, _ in cues], [0.0, 1.0])
        self.assertEqual([len(steps) for _, steps in cues], [1, 2])

    def test_an_empty_timeline_has_no_duration(self):
        self.assertEqual(duration_of([]), 0.0)


if __name__ == "__main__":
    unittest.main()
