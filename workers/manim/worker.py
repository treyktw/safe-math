#!/usr/bin/env python3
"""Isolated Manim renderer for the structured scene protocol.

The HTTP boundary accepts only structured Manim scenes. It deliberately does not
accept Python source, raw LaTeX, file paths, or scene class names. Rust
validates first; this process repeats the structural checks because callers
may reach the container directly.

Renders land on the shared media volume and are handed back as a path. The
caller can publish them to object storage afterwards from the same volume --
so a render that succeeds exists on disk even if the upload after it does
not, and no finished animation is ever held only in a response body.
"""

from __future__ import annotations

import hmac
import json
import logging
import os
import shutil
import tempfile
import threading
import uuid
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger("safe-manim")

MEDIA_ROOT = Path(os.getenv("MEDIA_ROOT", "/media"))
RENDER_SUBDIRECTORY = os.getenv("MANIM_RENDER_DIR", "renders")
PORT = int(os.getenv("MANIM_PORT", "8083"))
MAX_REQUEST_BYTES = int(os.getenv("MANIM_MAX_REQUEST_BYTES", str(4 * 1024 * 1024)))
RENDER_TIMEOUT_SECONDS = int(os.getenv("MANIM_RENDER_TIMEOUT_SECONDS", "600"))
QUEUE_TIMEOUT_SECONDS = int(os.getenv("MANIM_QUEUE_TIMEOUT_SECONDS", "120"))

# Rendering is CPU-bound and memory-hungry. Several at once on one box is how
# a render worker becomes the reason everything else on it is slow.
MAX_CONCURRENCY = int(os.getenv("MANIM_MAX_CONCURRENCY", "1"))

# Shared secret. Absent means the renderer trusts its network, which is true of
# a single-host deployment where nothing else can reach the port and false the
# moment that stops being true. Health is deliberately left open: a check that
# needs the secret goes red when the secret rotates, which is the opposite of
# what a health check is for.
TOKEN = os.getenv("MANIM_TOKEN") or ""

MAX_OBJECTS = 1_000
MAX_STEPS = 5_000
MAX_DURATION_SECONDS = 1_800.0
FORMATS = {"mp4": ".mp4", "webm": ".webm", "gif": ".gif"}

# Kept in step with ManimObject in the Rust protocol. An unknown kind is
# refused here rather than skipped during the build, because a scene whose
# objects were all skipped renders a blank video and reports success.
KINDS = {
    "text",
    "math_expression",
    "equation",
    "shape",
    "axes2d",
    "plot2d",
    "group",
}

slots = threading.Semaphore(MAX_CONCURRENCY)


class RenderError(Exception):
    """A failure with a code the Rust client already knows how to classify."""

    def __init__(self, status: int, code: str, message: str, retryable: bool = False):
        super().__init__(message)
        self.status = status
        self.code = code
        self.message = message
        self.retryable = retryable


def validate(request: Any) -> tuple[dict, dict]:
    """The structural checks, repeated on this side of the boundary."""
    if not isinstance(request, dict):
        raise RenderError(400, "manim_request_invalid", "a render request must be an object")

    scene = request.get("scene")
    if not isinstance(scene, dict):
        raise RenderError(400, "manim_scene_missing", "a render requires a scene")

    objects = scene.get("objects")
    if not isinstance(objects, list) or not objects or len(objects) > MAX_OBJECTS:
        raise RenderError(
            400,
            "manim_object_count_invalid",
            f"a scene requires 1 to {MAX_OBJECTS} objects",
        )

    for entry in objects:
        if not isinstance(entry, dict) or not entry.get("id"):
            raise RenderError(400, "manim_id_invalid", "every object requires an id")
        if entry.get("kind") not in KINDS:
            raise RenderError(
                400,
                "manim_object_kind_unknown",
                f"unsupported object kind {entry.get('kind')!r}",
            )

    timeline = scene.get("timeline") or []
    if not isinstance(timeline, list) or len(timeline) > MAX_STEPS:
        raise RenderError(
            400,
            "manim_timeline_too_large",
            f"a scene supports at most {MAX_STEPS} animation steps",
        )

    output = request.get("output")
    output = output if isinstance(output, dict) else {}
    fmt = str(output.get("format", "mp4"))
    if fmt not in FORMATS:
        raise RenderError(400, "manim_output_invalid", f"unsupported output format {fmt}")
    if output.get("transparent") and fmt == "mp4":
        raise RenderError(
            400, "manim_output_invalid", "mp4 cannot carry transparency; use webm"
        )

    width = int(output.get("width", 1920))
    height = int(output.get("height", 1080))
    fps = int(output.get("framesPerSecond", output.get("frames_per_second", 30)))
    if not (320 <= width <= 7680 and 240 <= height <= 4320 and 12 <= fps <= 120):
        raise RenderError(
            400,
            "manim_output_invalid",
            "output dimensions or frame rate are unsupported",
        )

    from scene import duration_of

    if duration_of(timeline) > MAX_DURATION_SECONDS:
        raise RenderError(
            400, "manim_duration_too_long", "a Manim scene may be at most 30 minutes"
        )

    return scene, {
        "width": width,
        "height": height,
        "framesPerSecond": fps,
        "format": fmt,
        "transparent": bool(output.get("transparent")),
    }


def render(request: dict) -> dict:
    """Render one scene and report where it landed."""
    from manim import tempconfig

    from scene import build_scene_class, duration_of

    scene_spec, output = validate(request)

    job_id = str(uuid.uuid4())
    destination = MEDIA_ROOT / RENDER_SUBDIRECTORY / job_id
    destination.mkdir(parents=True, exist_ok=True)

    # Manim writes a tree of its own. It is built somewhere disposable and only
    # the finished file is moved into place, because a partial file under the
    # name the worker was told to publish would be published.
    workdir = Path(tempfile.mkdtemp(prefix="manim-"))

    settings = {
        "media_dir": str(workdir),
        "pixel_width": output["width"],
        "pixel_height": output["height"],
        "frame_rate": output["framesPerSecond"],
        "output_file": "scene",
        "disable_caching": True,
        "verbosity": "ERROR",
        "progress_bar": "none",
        "transparent": output["transparent"],
        "format": output["format"],
    }

    try:
        with tempconfig(settings):
            scene = build_scene_class(request)()
            scene.render()
            produced = produced_file(scene.renderer.file_writer, output["format"])

        if not produced.exists():
            raise RenderError(
                500,
                "manim_output_missing",
                "the renderer reported success and wrote no file",
                retryable=True,
            )

        final = destination / f"scene{produced.suffix}"
        shutil.move(str(produced), final)
    except RenderError:
        shutil.rmtree(destination, ignore_errors=True)
        raise
    except Exception as error:
        shutil.rmtree(destination, ignore_errors=True)
        # A scene the renderer cannot draw is the caller's input, not a fault
        # in this process -- retrying it would fail identically.
        raise RenderError(
            422, "manim_render_failed", f"{type(error).__name__}: {error}"[:500]
        ) from error
    finally:
        shutil.rmtree(workdir, ignore_errors=True)

    return {
        "jobId": job_id,
        "status": "completed",
        "renderer": f"manim-{version()}",
        "artifact": {
            # Relative to the shared volume, because the worker mounts it at a
            # path of its own and an absolute path here would be this
            # container's view of the world.
            "path": str(final.relative_to(MEDIA_ROOT)),
            "format": output["format"],
            "width": output["width"],
            "height": output["height"],
            "framesPerSecond": output["framesPerSecond"],
            "durationSeconds": duration_of(scene_spec.get("timeline") or []),
            "sizeBytes": final.stat().st_size,
        },
    }


def produced_file(file_writer: Any, fmt: str) -> Path:
    """Where Manim actually wrote the render.

    A GIF is written to `gif_file_path`; `movie_file_path` still names the
    .mp4 it would have written, which does not exist. Reading the movie path
    for a GIF reports "wrote no file" for a render that succeeded.
    """
    if fmt == "gif":
        gif_path = getattr(file_writer, "gif_file_path", None)
        if gif_path:
            return Path(gif_path)
    return Path(file_writer.movie_file_path)


def version() -> str:
    try:
        import manim

        return str(manim.__version__)
    except Exception:
        return "unknown"


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "safe-manim"

    def log_message(self, fmt: str, *args) -> None:  # noqa: A003
        log.info("%s", fmt % args)

    def _send(self, status: int, body: dict) -> None:
        payload = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _fail(self, error: RenderError) -> None:
        self._send(
            error.status,
            {"code": error.code, "message": error.message, "retryable": error.retryable},
        )

    def do_GET(self) -> None:  # noqa: N802
        if self.path.rstrip("/") not in {"/health", ""}:
            self._fail(RenderError(404, "manim_not_found", "no such endpoint"))
            return
        self._send(
            HTTPStatus.OK,
            {
                "status": "ok",
                "service": "safe-manim",
                "renderer": f"manim-{version()}",
                "maxConcurrency": MAX_CONCURRENCY,
                "renderTimeoutSeconds": RENDER_TIMEOUT_SECONDS,
            },
        )

    def _authorized(self) -> bool:
        if not TOKEN:
            return True
        offered = self.headers.get("authorization") or ""
        scheme, _, presented = offered.partition(" ")
        # Constant time, because a comparison that returns early on the first
        # wrong byte tells the caller how much of the secret it guessed.
        return scheme.lower() == "bearer" and hmac.compare_digest(presented.strip(), TOKEN)

    def do_POST(self) -> None:  # noqa: N802
        if self.path.rstrip("/") != "/v1/renders":
            self._fail(RenderError(404, "manim_not_found", "no such endpoint"))
            return

        if not self._authorized():
            self._fail(RenderError(401, "manim_unauthorized", "a render requires the renderer token"))
            return

        length = int(self.headers.get("content-length") or 0)
        if length > MAX_REQUEST_BYTES:
            self._fail(
                RenderError(413, "manim_request_too_large", "render request exceeds 4 MiB")
            )
            return

        try:
            request = json.loads(self.rfile.read(length) or b"{}")
        except json.JSONDecodeError as error:
            self._fail(RenderError(400, "manim_request_malformed", str(error)))
            return

        # Queued rather than refused while busy: a caller that already waited
        # for a model to compose the scene should not be told to come back.
        if not slots.acquire(timeout=QUEUE_TIMEOUT_SECONDS):
            self._fail(
                RenderError(
                    429, "manim_renderer_busy", "the renderer is at capacity", retryable=True
                )
            )
            return

        try:
            result = render(request)
        except RenderError as error:
            log.warning("render rejected: %s %s", error.code, error.message)
            self._fail(error)
            return
        except Exception as error:  # noqa: BLE001
            log.exception("render failed")
            self._fail(
                RenderError(
                    500, "manim_renderer_failed", str(error)[:500], retryable=True
                )
            )
            return
        finally:
            slots.release()

        log.info(
            "rendered %s (%s bytes)", result["artifact"]["path"], result["artifact"]["sizeBytes"]
        )
        self._send(HTTPStatus.OK, result)


def main() -> None:
    (MEDIA_ROOT / RENDER_SUBDIRECTORY).mkdir(parents=True, exist_ok=True)
    log.info(
        "safe-manim on %s, writing to %s, concurrency %s, auth %s",
        PORT,
        MEDIA_ROOT / RENDER_SUBDIRECTORY,
        MAX_CONCURRENCY,
        "required" if TOKEN else "off (trusting the network)",
    )
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
