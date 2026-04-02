#!/usr/bin/env python3
"""Lightweight HTTP fixture backend for Linux smoke tests and baselines."""

from __future__ import annotations

import argparse
import json
import signal
import sys
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a lightweight HTTP fixture backend for gateway validation."
    )
    parser.add_argument("--bind", default="127.0.0.1", help="listen address")
    parser.add_argument("--port", type=int, default=19000, help="listen port")
    parser.add_argument(
        "--default-bytes",
        type=int,
        default=1024,
        help="response size used by /fixture/default",
    )
    return parser.parse_args()


class FixtureHandler(BaseHTTPRequestHandler):
    server_version = "RivuletFixture/0.1"
    protocol_version = "HTTP/1.1"

    def do_GET(self) -> None:
        self._handle_request(send_body=True)

    def do_HEAD(self) -> None:
        self._handle_request(send_body=False)

    def log_message(self, fmt: str, *args: object) -> None:
        message = {
            "remote": self.client_address[0],
            "request": self.requestline,
            "message": fmt % args,
        }
        sys.stdout.write(json.dumps(message, ensure_ascii=False) + "\n")
        sys.stdout.flush()

    def _handle_request(self, send_body: bool) -> None:
        parsed = urlparse(self.path)
        path = parsed.path

        if path == "/healthz":
            self._write_response(HTTPStatus.OK, b"ok\n", "text/plain; charset=utf-8", send_body)
            return

        if path == "/fixture/default":
            body = b"x" * self.server.default_bytes
            self._write_response(HTTPStatus.OK, body, "application/octet-stream", send_body)
            return

        prefix = "/fixture/bytes/"
        if path.startswith(prefix):
            size_text = path[len(prefix) :]
            if not size_text.isdigit():
                self._write_response(
                    HTTPStatus.BAD_REQUEST,
                    b"invalid size\n",
                    "text/plain; charset=utf-8",
                    send_body,
                )
                return

            size = int(size_text)
            if size < 0 or size > 8 * 1024 * 1024:
                self._write_response(
                    HTTPStatus.BAD_REQUEST,
                    b"size out of range\n",
                    "text/plain; charset=utf-8",
                    send_body,
                )
                return

            body = b"x" * size
            self._write_response(HTTPStatus.OK, body, "application/octet-stream", send_body)
            return

        if path == "/fixture/json":
            body = json.dumps(
                {
                    "path": path,
                    "method": self.command,
                    "host": self.headers.get("Host"),
                },
                ensure_ascii=False,
            ).encode("utf-8")
            self._write_response(HTTPStatus.OK, body, "application/json; charset=utf-8", send_body)
            return

        self._write_response(
            HTTPStatus.NOT_FOUND,
            b"fixture route not found\n",
            "text/plain; charset=utf-8",
            send_body,
        )

    def _write_response(
        self,
        status: HTTPStatus,
        body: bytes,
        content_type: str,
        send_body: bool,
    ) -> None:
        self.send_response(status.value, status.phrase)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        if send_body:
            self.wfile.write(body)


class FixtureServer(ThreadingHTTPServer):
    def __init__(self, server_address: tuple[str, int], default_bytes: int) -> None:
        super().__init__(server_address, FixtureHandler)
        self.default_bytes = default_bytes


def main() -> int:
    args = parse_args()
    server = FixtureServer((args.bind, args.port), args.default_bytes)

    def handle_signal(_signum: int, _frame: object) -> None:
        server.shutdown()

    signal.signal(signal.SIGTERM, handle_signal)
    signal.signal(signal.SIGINT, handle_signal)

    print(
        json.dumps(
            {
                "event": "fixture-backend-started",
                "bind": args.bind,
                "port": args.port,
                "default_bytes": args.default_bytes,
            },
            ensure_ascii=False,
        )
    )
    sys.stdout.flush()
    server.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
