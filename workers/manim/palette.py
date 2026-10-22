"""Theme tokens, resolved to colours the renderer understands.

The protocol lets a colour be a hex value or a theme token, because the rest
of the product names colours rather than spelling them (a shared theme). A render
is the one place those names have to become numbers, so the mapping lives here
and nowhere else -- a scene that says `ember` should look like a citation does
in the app.

Dark values only: an animation is a file, not a page, and it cannot re-resolve
itself when the viewer switches theme.
"""

from __future__ import annotations

# Mirrors the app's dark theme tokens. Alpha is
# dropped -- Manim composites against the scene background, and a colour that
# is 4.5% opaque is not a colour anyone asked to see.
TOKENS = {
    "background": "#0A0B0E",
    "void": "#0A0B0E",
    "surface": "#14161A",
    "raised": "#1C1F25",
    "foreground": "#E9E9ED",
    "text": "#E9E9ED",
    "muted": "#93939A",
    "text-muted": "#93939A",
    "faint": "#5B5B62",
    "text-faint": "#5B5B62",
    "current": "#A9B2BF",
    "current-dim": "#7D8794",
    "accent": "#A9B2BF",
    "primary": "#A9B2BF",
    "ember": "#C6CCD6",
    "aurora": "#BAA0DF",
    "danger": "#F2555A",
    "destructive": "#F2555A",
    "success": "#4ECB8D",
    "border": "#2A2D34",
    "hairline": "#2A2D34",
}

FALLBACK = TOKENS["foreground"]


def resolve(colour, default: str | None = None) -> str:
    """A hex string for a protocol colour.

    Order matters: a token wins over Manim's own constants, so a scene that
    says `background` gets the theme's near-black rather than Manim's, and the
    two never drift apart.
    """
    if not isinstance(colour, str) or not colour:
        return default or FALLBACK

    if colour.startswith("#"):
        return _expand(colour) or (default or FALLBACK)

    token = TOKENS.get(colour.strip().lower().replace("_", "-"))
    if token:
        return token

    # Manim's own vocabulary -- BLUE, RED, YELLOW_E. Accepted because a model
    # writing a scene reaches for those names, and refusing them would show up
    # as a colourless render rather than as an error anyone reads.
    named = _manim_colour(colour)
    return named or default or FALLBACK


def _expand(value: str) -> str | None:
    digits = value[1:]
    if len(digits) in (3, 4):
        digits = "".join(digit * 2 for digit in digits)
    if len(digits) not in (6, 8):
        return None
    if any(character not in "0123456789abcdefABCDEF" for character in digits):
        return None
    # Manim's parser wants RGB; alpha is carried by opacity, not by the hex.
    return f"#{digits[:6].upper()}"


def _manim_colour(name: str) -> str | None:
    try:
        from manim import ManimColor
        import manim

        constant = getattr(manim, name.strip().upper().replace("-", "_"), None)
        if isinstance(constant, ManimColor):
            return constant.to_hex().upper()
    except Exception:
        pass
    return None
