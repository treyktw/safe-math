# The render sidecar: Manim, behind the structured scene protocol.
#
# Built on the community image rather than assembled here. Manim needs a TeX
# distribution, Cairo, Pango and ffmpeg, and assembling that set correctly is a
# maintenance job somebody upstream is already doing.
#
# Separate service for the same reason transcription is: rendering is
# CPU-bound and long-running, and a render occupying a worker slot would stall
# the document and media work that shares it.
FROM manimcommunity/manim:v0.19.0

USER root

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    MEDIA_ROOT=/media \
    MANIM_PORT=8083 \
    MANIM_MAX_CONCURRENCY=1 \
    MANIM_RENDER_TIMEOUT_SECONDS=600

# uid 10001 is not arbitrary: in production it matches the application
# worker's uid. This container and the worker share one named volume, and a
# render written under a different uid is a file the worker can see and cannot
# publish. Matching the uid is what makes the handoff work without opening the
# volume to everyone.
RUN useradd --system --uid 10001 --create-home render \
    && mkdir -p /media/renders \
    && chown -R render:render /media

WORKDIR /app
COPY --chmod=0644 latex.py   ./latex.py
COPY --chmod=0644 palette.py ./palette.py
COPY --chmod=0644 scene.py   ./scene.py
COPY --chmod=0644 worker.py  ./worker.py

USER render
EXPOSE 8083

# Long start period: the first import builds Manim's font and TeX caches, and a
# container killed for being slow to start would never finish building them.
HEALTHCHECK --interval=30s --timeout=5s --start-period=90s --retries=3 \
  CMD ["python", "-c", "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8083/health', timeout=3).read()"]

ENTRYPOINT ["python", "-u", "/app/worker.py"]
