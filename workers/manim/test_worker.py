"""The checks that stand on their own at the HTTP boundary.

Rust validates first. These run anyway, because the container is reachable
without going through Rust and a renderer that trusts its caller is a renderer
that executes whatever reaches the port.
"""

import unittest

import test_scene  # noqa: F401  -- installs the Manim stub before worker imports it
import worker
from worker import RenderError, validate


def scene(**overrides):
    request = {
        "scene": {
            "backgroundColor": "background",
            "objects": [{"kind": "text", "id": "t", "text": "hello", "position": {"x": 0, "y": 0}}],
            "timeline": [{"startSeconds": 0.0, "durationSeconds": 1.0, "action": {"action": "write", "object_id": "t"}}],
        },
        "output": {"width": 1920, "height": 1080, "framesPerSecond": 30, "format": "mp4"},
    }
    request.update(overrides)
    return request


class ValidationTests(unittest.TestCase):
    def refuse(self, request) -> str:
        with self.assertRaises(RenderError) as caught:
            validate(request)
        return caught.exception.code

    def test_accepts_a_scene_carrying_no_python_and_no_latex(self):
        _, output = validate(scene())
        self.assertEqual(output["format"], "mp4")
        self.assertEqual(output["framesPerSecond"], 30)

    def test_reads_the_snake_case_spelling_of_the_frame_rate(self):
        _, output = validate(scene(output={"frames_per_second": 24}))
        self.assertEqual(output["framesPerSecond"], 24)

    def test_a_request_without_a_scene_is_refused(self):
        self.assertEqual(self.refuse({}), "manim_scene_missing")

    def test_an_empty_scene_is_refused(self):
        request = scene()
        request["scene"]["objects"] = []
        self.assertEqual(self.refuse(request), "manim_object_count_invalid")

    def test_mp4_cannot_carry_transparency(self):
        # ffmpeg would encode it opaque and report success, so the alpha the
        # caller asked for disappears with nothing to read about it.
        self.assertEqual(
            self.refuse(scene(output={"format": "mp4", "transparent": True})),
            "manim_output_invalid",
        )

    def test_an_unlisted_container_format_is_refused(self):
        self.assertEqual(self.refuse(scene(output={"format": "mkv"})), "manim_output_invalid")

    def test_absurd_dimensions_are_refused(self):
        self.assertEqual(
            self.refuse(scene(output={"width": 100000, "height": 1080})),
            "manim_output_invalid",
        )

    def test_a_scene_longer_than_the_cap_is_refused(self):
        request = scene()
        request["scene"]["timeline"] = [{"startSeconds": 0.0, "durationSeconds": 3600.0}]
        self.assertEqual(self.refuse(request), "manim_duration_too_long")

    def test_an_object_kind_it_cannot_build_is_refused_not_skipped(self):
        # Skipping it renders a blank video and calls it a success, which the
        # pipeline then publishes as the animation somebody asked for.
        request = scene()
        request["scene"]["objects"] = [{"kind": "future_thing", "id": "a"}]
        self.assertEqual(self.refuse(request), "manim_object_kind_unknown")

    def test_an_object_without_an_id_is_refused(self):
        request = scene()
        request["scene"]["objects"] = [{"kind": "text", "text": "x"}]
        self.assertEqual(self.refuse(request), "manim_id_invalid")

    def test_defaults_stand_in_for_an_absent_output_block(self):
        request = scene()
        del request["output"]
        _, output = validate(request)
        self.assertEqual((output["width"], output["height"]), (1920, 1080))


class ProducedFileTests(unittest.TestCase):
    """Which path the renderer reads back after Manim finishes."""

    class Writer:
        movie_file_path = "/tmp/render/videos/scene.mp4"
        gif_file_path = "/tmp/render/videos/scene.gif"

    def test_a_gif_is_read_from_the_gif_path(self):
        # Manim leaves movie_file_path naming an .mp4 that was never written.
        self.assertEqual(worker.produced_file(self.Writer(), "gif").suffix, ".gif")

    def test_video_formats_read_the_movie_path(self):
        self.assertEqual(worker.produced_file(self.Writer(), "mp4").suffix, ".mp4")
        self.assertEqual(worker.produced_file(self.Writer(), "webm").suffix, ".mp4")


class AuthorizationTests(unittest.TestCase):
    """The token check, without standing up a socket."""

    class Request:
        def __init__(self, header=None):
            self.headers = {} if header is None else {"authorization": header}

        def get(self, name):
            return self.headers.get(name)

    def authorized(self, token, header):
        original = worker.TOKEN
        worker.TOKEN = token
        try:
            handler = worker.Handler.__new__(worker.Handler)
            handler.headers = self.Request(header).headers
            return worker.Handler._authorized(handler)
        finally:
            worker.TOKEN = original

    def test_no_token_configured_trusts_the_network(self):
        self.assertTrue(self.authorized("", None))

    def test_the_right_token_is_accepted(self):
        self.assertTrue(self.authorized("s3cret", "Bearer s3cret"))

    def test_the_scheme_is_matched_case_insensitively(self):
        self.assertTrue(self.authorized("s3cret", "bearer s3cret"))

    def test_a_wrong_token_is_refused(self):
        self.assertFalse(self.authorized("s3cret", "Bearer wrong"))

    def test_a_missing_header_is_refused_once_a_token_exists(self):
        self.assertFalse(self.authorized("s3cret", None))

    def test_the_token_alone_is_not_a_bearer_header(self):
        self.assertFalse(self.authorized("s3cret", "s3cret"))


if __name__ == "__main__":
    unittest.main()
