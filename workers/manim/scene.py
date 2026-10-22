"""A scene description, built into Manim objects.

Nothing here evaluates anything the caller sent. Objects are looked up in a
table by kind and constructed with typed arguments; an unrecognised kind is
skipped. That is the whole reason the protocol carries a tree rather than
Python -- a render must not be able to become execution.
"""

from __future__ import annotations

import math
from typing import Any

from manim import (
    Arrow,
    Axes,
    Brace,
    Circle,
    Create,
    Dot,
    FadeIn,
    FadeOut,
    Line,
    MathTex,
    Rectangle,
    Scene,
    Square,
    Text,
    Transform,
    Triangle,
    VGroup,
    Write,
    linear,
    rate_functions,
    smooth,
)

from latex import equation_to_latex, to_latex
from palette import resolve as colour

EASING = {
    "linear": linear,
    "smooth": smooth,
    "ease_in": rate_functions.ease_in_sine,
    "ease_out": rate_functions.ease_out_sine,
    "ease_in_out": rate_functions.ease_in_out_sine,
}

# The shortest animation worth rendering. A zero-length play would emit no
# frames and read as a step that silently did not happen.
MIN_DURATION = 0.05


def field(spec: Any, *names: str, default: Any = None) -> Any:
    """One value, under whichever spelling arrived.

    Serde renames enum variants but not the fields inside them, so the wire
    carries `objectId` in some places and `object_id` in others. Rather than
    encode which is which -- and re-encode it every time the Rust protocol
    gains a field -- both spellings are accepted.
    """
    if not isinstance(spec, dict):
        return default
    for name in names:
        if name in spec and spec[name] is not None:
            return spec[name]
    return default


def _point(position: Any) -> list[float]:
    if not isinstance(position, dict):
        return [0.0, 0.0, 0.0]
    return [
        float(position.get("x", 0.0)),
        float(position.get("y", 0.0)),
        float(position.get("z", 0.0)),
    ]


def _styled(mobject, style: Any):
    """Colour, opacity and scale, where the object supports them.

    Applied defensively: Manim's objects do not all take the same setters, and
    a style that a shape cannot honour should leave the shape rather than fail
    the render.
    """
    if not isinstance(style, dict):
        return mobject

    stroke = field(style, "color")
    if stroke:
        try:
            mobject.set_color(colour(stroke))
        except Exception:
            pass

    fill = field(style, "fillColor", "fill_color")
    if fill:
        try:
            mobject.set_fill(colour(fill), opacity=float(style.get("opacity", 1.0)))
        except Exception:
            pass

    width = field(style, "strokeWidth", "stroke_width")
    if width is not None:
        try:
            mobject.set_stroke(width=float(width))
        except Exception:
            pass

    scale = float(style.get("scale", 1.0) or 1.0)
    if scale != 1.0:
        try:
            mobject.scale(scale)
        except Exception:
            pass

    return mobject


def build_objects(objects: list[dict]) -> dict[str, Any]:
    """Every object in the scene, by id.

    Plots and groups are resolved after the rest, because both refer to other
    objects by id and those have to exist first.
    """
    built: dict[str, Any] = {}
    deferred: list[dict] = []

    for spec in objects:
        if not isinstance(spec, dict):
            continue
        kind = spec.get("kind")
        identifier = spec.get("id")
        if not identifier:
            continue

        try:
            if kind == "text":
                mobject = Text(str(spec.get("text", "")))
            elif kind == "math_expression":
                mobject = MathTex(to_latex(spec.get("expression")))
            elif kind == "equation":
                mobject = MathTex(equation_to_latex(spec.get("equation")))
            elif kind == "shape":
                mobject = _shape(spec)
            elif kind == "axes2d":
                mobject = _axes(spec)
            elif kind in {"plot2d", "group"}:
                deferred.append(spec)
                continue
            else:
                # An unknown kind is skipped rather than guessed at.
                continue
        except Exception:
            # One object that cannot be built should not lose the scene.
            continue

        mobject.move_to(_point(spec.get("position")))
        built[identifier] = _styled(mobject, spec.get("style"))

    for spec in deferred:
        identifier = spec.get("id")
        try:
            if spec.get("kind") == "group":
                members = [built[m] for m in spec.get("members", []) if m in built]
                if members:
                    built[identifier] = VGroup(*members)
                continue

            axes = built.get(field(spec, "axes_id", "axesId"))
            if axes is None:
                continue
            built[identifier] = _styled(_plot(axes, spec), spec.get("style"))
        except Exception:
            continue

    return built


def _shape(spec: dict):
    kind = spec.get("shape")
    width = float(spec.get("width", 1.0))
    height = float(spec.get("height", 1.0))

    if kind == "circle":
        return Circle(radius=max(width, height) / 2)
    if kind == "square":
        return Square(side_length=width)
    if kind == "rectangle":
        return Rectangle(width=width, height=height)
    if kind == "triangle":
        return Triangle().scale(max(width, height) / 2)
    if kind == "line":
        return Line([0, 0, 0], [width, height, 0])
    if kind == "arrow":
        return Arrow([0, 0, 0], [width, height, 0])
    if kind == "dot":
        return Dot()
    if kind == "brace":
        return Brace(Line([0, 0, 0], [width, 0, 0]))
    raise ValueError("unknown shape")


def _axes(spec: dict):
    x_range = field(spec, "x_range", "xRange", default=[-5, 5])
    y_range = field(spec, "y_range", "yRange", default=[-5, 5])
    return Axes(
        x_range=[float(x_range[0]), float(x_range[1])],
        y_range=[float(y_range[0]), float(y_range[1])],
        x_length=float(spec.get("width", 6.0)),
        y_length=float(spec.get("height", 4.0)),
    )


def _plot(axes, spec: dict):
    """A curve, sampled from the expression tree.

    The tree is walked per point rather than compiled: `eval` on anything the
    caller sent would undo the reason this protocol is structured.
    """
    variable = str(spec.get("variable", "x"))
    domain = spec.get("domain") or [-5, 5]
    expression = spec.get("expression")

    def value(at: float) -> float:
        try:
            result = evaluate(expression, {variable: at})
        except Exception:
            # A pole or a domain error is a gap in the curve, not a failure.
            return 0.0
        return result if math.isfinite(result) else 0.0

    return axes.plot(value, x_range=[float(domain[0]), float(domain[1])])


CONSTANTS = {"pi": math.pi, "e": math.e}

FUNCTIONS = {
    "sin": math.sin,
    "cos": math.cos,
    "tan": math.tan,
    "asin": math.asin,
    "acos": math.acos,
    "atan": math.atan,
    "sinh": math.sinh,
    "cosh": math.cosh,
    "tanh": math.tanh,
    "exp": math.exp,
    "ln": math.log,
    "log": math.log10,
    "sqrt": math.sqrt,
    "abs": abs,
    "floor": math.floor,
    "ceiling": math.ceil,
    "gamma": math.gamma,
    "min": min,
    "max": max,
}


def evaluate(node: Any, bindings: dict[str, float]) -> float:
    """Evaluate an expression tree numerically.

    A small interpreter rather than `eval`. Every operation is one this file
    names, so nothing the caller sends can do more than arithmetic.
    """
    if not isinstance(node, dict):
        raise ValueError("expression node is not an object")
    kind = node.get("kind")

    if kind in {"integer", "decimal"}:
        return float(node.get("value", 0))
    if kind == "symbol":
        name = str(node.get("name", ""))
        if name not in bindings:
            raise ValueError(f"unbound symbol {name}")
        return float(bindings[name])
    if kind == "constant":
        name = str(node.get("value", ""))
        if name not in CONSTANTS:
            # `i` and `infinity` are in the AST but are not points on a curve.
            raise ValueError(f"{name} is not a plottable constant")
        return CONSTANTS[name]
    if kind == "negate":
        return -evaluate(node.get("value"), bindings)
    if kind == "add":
        return float(sum(evaluate(term, bindings) for term in node.get("terms") or []))
    if kind == "multiply":
        product = 1.0
        for factor in node.get("factors") or []:
            product *= evaluate(factor, bindings)
        return product
    if kind == "divide":
        denominator = evaluate(node.get("denominator"), bindings)
        if denominator == 0:
            raise ZeroDivisionError
        return evaluate(node.get("numerator"), bindings) / denominator
    if kind == "power":
        return float(
            evaluate(node.get("base"), bindings) ** evaluate(node.get("exponent"), bindings)
        )
    if kind == "function":
        name = str(node.get("function", ""))
        function = FUNCTIONS.get(name)
        if function is None:
            if name == "factorial":
                return float(math.factorial(int(evaluate(node["arguments"][0], bindings))))
            raise ValueError(f"unsupported function {name}")
        return float(function(*(evaluate(a, bindings) for a in node.get("arguments") or [])))

    raise ValueError(f"unsupported node {kind}")


def duration_of(timeline: list) -> float:
    """How long the scene runs, from its own timing."""
    end = 0.0
    for step in timeline:
        if not isinstance(step, dict):
            continue
        start = float(field(step, "startSeconds", "start_seconds", default=0.0) or 0.0)
        length = float(field(step, "durationSeconds", "duration_seconds", default=0.0) or 0.0)
        end = max(end, start + length)
    return end


def _cues(timeline: list) -> list[tuple[float, list[dict]]]:
    """Timeline steps, grouped by the moment they start.

    `startSeconds` is a wall clock, not an ordering: two steps that share a
    start were meant to happen together, and playing them one after the other
    would stretch the scene and desynchronise everything after it.
    """
    grouped: dict[float, list[dict]] = {}
    for step in timeline:
        if not isinstance(step, dict):
            continue
        start = float(field(step, "startSeconds", "start_seconds", default=0.0) or 0.0)
        grouped.setdefault(round(start, 4), []).append(step)
    return sorted(grouped.items())


def build_scene_class(request: dict):
    """A Manim Scene that plays this timeline."""
    scene_spec = request.get("scene") or {}
    objects = scene_spec.get("objects") or []
    timeline = scene_spec.get("timeline") or []
    background = colour(
        field(scene_spec, "backgroundColor", "background_color"), "#0A0B0E"
    )

    class RequestedScene(Scene):
        def construct(self) -> None:
            self.camera.background_color = background
            built = build_objects(objects)
            if not built:
                # Every object failed to build. Rendering anyway produces a
                # blank video that the pipeline would publish as a success.
                raise ValueError("no object in the scene could be built")

            # A scene that played nothing would render zero frames and be
            # reported as a success with no file behind it.
            if not timeline:
                if built:
                    self.add(*built.values())
                self.wait(1)
                return

            clock = 0.0
            for start, steps in _cues(timeline):
                if start > clock:
                    self.wait(start - clock)
                    clock = start
                clock += self._play(built, steps)

        def _play(self, built: dict, steps: list[dict]) -> float:
            """Play one cue. Returns how much of the clock it consumed."""
            run_time = max(
                (
                    float(field(s, "durationSeconds", "duration_seconds", default=0.0) or 0.0)
                    for s in steps
                ),
                default=MIN_DURATION,
            )
            run_time = max(run_time, MIN_DURATION)
            rate = EASING.get(str(steps[0].get("easing", "smooth")), smooth)

            animations = []
            for step in steps:
                try:
                    animation = self._animation(built, step.get("action") or {})
                except Exception:
                    animation = None
                if animation is not None:
                    animations.append(animation)

            if not animations:
                # A cue of pure waits, or one whose objects failed to build.
                # It still costs its own time, so the rest stays in sync.
                self.wait(run_time)
                return run_time

            try:
                self.play(*animations, run_time=run_time, rate_func=rate)
            except Exception:
                # One cue that will not play should cost its own time, not the
                # rest of the animation.
                self.wait(run_time)
            return run_time

        def _animation(self, built: dict, action: dict):
            kind = action.get("action") or action.get("kind")
            target = built.get(field(action, "object_id", "objectId"))

            if kind == "wait" or kind == "camera":
                # The camera only moves in Manim's 3D and moving-camera scenes.
                # Honouring it here would mean rendering every scene in the
                # more expensive class for a frame nobody asked to move.
                return None
            if kind == "transform":
                source = built.get(field(action, "from_id", "fromId"))
                destination = built.get(field(action, "to_id", "toId"))
                if source is None or destination is None:
                    return None
                return Transform(source, destination)
            if target is None:
                return None
            if kind == "create":
                return Create(target)
            if kind == "write":
                return Write(target)
            if kind == "fade_in":
                return FadeIn(target)
            if kind == "fade_out":
                return FadeOut(target)
            if kind == "move_to":
                return target.animate.move_to(_point(action.get("position")))
            if kind == "highlight":
                return target.animate.set_color(colour(action.get("color"), "#BAA0DF"))
            return None

    return RequestedScene
