"""A real render, end to end inside this process.

Skipped unless MANIM_RENDER_TESTS=1, because it needs Manim itself plus Cairo,
Pango, ffmpeg and a LaTeX distribution. Every other test in this directory
stubs Manim out and runs with pytest alone.

    MANIM_RENDER_TESTS=1 python -m pytest test_render.py
"""

import os
import tempfile
import unittest
from pathlib import Path

ENABLED = os.getenv("MANIM_RENDER_TESTS") == "1"


@unittest.skipUnless(ENABLED, "set MANIM_RENDER_TESTS=1 with Manim and LaTeX installed")
class RenderTests(unittest.TestCase):
    def setUp(self):
        import manim

        if not hasattr(manim, "__version__"):
            self.skipTest("the Manim stub from test_scene is loaded; run this file on its own")
        import worker

        self.worker = worker
        self.media = tempfile.TemporaryDirectory()
        self.original_root = worker.MEDIA_ROOT
        worker.MEDIA_ROOT = Path(self.media.name)

    def tearDown(self):
        self.worker.MEDIA_ROOT = self.original_root
        self.media.cleanup()

    def request(self, fmt):
        return {
            "scene": {
                "backgroundColor": "background",
                "objects": [
                    {
                        "kind": "equation",
                        "id": "eq",
                        "equation": {
                            "left": {
                                "kind": "power",
                                "base": {"kind": "symbol", "name": "x"},
                                "exponent": {"kind": "integer", "value": "2"},
                            },
                            "right": {"kind": "integer", "value": "4"},
                        },
                        "position": {"x": 0, "y": 0},
                    }
                ],
                "timeline": [
                    {
                        "startSeconds": 0.0,
                        "durationSeconds": 0.5,
                        "action": {"action": "write", "object_id": "eq"},
                    }
                ],
            },
            "output": {"width": 320, "height": 240, "framesPerSecond": 12, "format": fmt},
        }

    def test_renders_an_mp4_onto_the_media_root(self):
        result = self.worker.render(self.request("mp4"))
        path = Path(self.media.name) / result["artifact"]["path"]
        self.assertTrue(path.exists())
        self.assertEqual(path.suffix, ".mp4")

    def test_renders_a_gif_where_manim_actually_writes_it(self):
        result = self.worker.render(self.request("gif"))
        path = Path(self.media.name) / result["artifact"]["path"]
        self.assertTrue(path.exists())
        self.assertEqual(path.suffix, ".gif")


if __name__ == "__main__":
    unittest.main()
