FROM python:3.13-slim-bookworm

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    MATH_PORT=8079 \
    MATH_MAX_CONCURRENCY=2 \
    MATH_OPERATION_TIMEOUT_SECONDS=15

WORKDIR /app
COPY requirements.lock ./requirements.lock
RUN python -m pip install --no-cache-dir --require-hashes -r requirements.lock \
    && useradd --system --uid 10002 --create-home math

COPY --chmod=0644 worker.py ./worker.py

USER math
EXPOSE 8079

HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=3 \
  CMD ["python", "-c", "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8079/health', timeout=2).read()"]

CMD ["python", "-u", "/app/worker.py"]
