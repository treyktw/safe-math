"""Theme tokens have to survive the trip into a video file."""

import unittest

from palette import FALLBACK, resolve


class PaletteTests(unittest.TestCase):
    def test_a_token_resolves_to_the_theme_value(self):
        self.assertEqual(resolve("background"), "#0A0B0E")
        self.assertEqual(resolve("ember"), "#C6CCD6")

    def test_tokens_are_matched_however_they_are_spelled(self):
        self.assertEqual(resolve("TEXT_MUTED"), resolve("text-muted"))

    def test_short_hex_is_expanded(self):
        self.assertEqual(resolve("#abc"), "#AABBCC")

    def test_alpha_is_dropped_rather_than_passed_to_a_parser_that_wants_rgb(self):
        self.assertEqual(resolve("#0A0B0EFF"), "#0A0B0E")

    def test_a_malformed_hex_falls_back_instead_of_reaching_the_renderer(self):
        self.assertEqual(resolve("#gggggg"), FALLBACK)

    def test_an_unknown_name_falls_back_to_the_foreground(self):
        self.assertEqual(resolve("chartreuse-supreme"), FALLBACK)

    def test_a_caller_supplied_default_wins_over_the_global_fallback(self):
        self.assertEqual(resolve(None, "#123456"), "#123456")


if __name__ == "__main__":
    unittest.main()
