"""Serves the repository on http://localhost:8765 for the browser tests
(tests/web/harness.html) and logs every request with its method, path and
body size, so a test can confirm the page sends nothing back.

Usage: python3 tools/web_test_server.py [LOGFILE]
"""

import http.server
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOG = open(sys.argv[1] if len(sys.argv) > 1 else "/dev/stdout", "a", buffering=1)


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm",
                      ".js": "text/javascript"}

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT), **kwargs)

    def log_message(self, fmt, *args):
        pass

    def log_request(self, code="-", size="-"):
        if self.path.startswith("/__harness-result?"):
            return
        length = self.headers.get("Content-Length", "0")
        LOG.write(f"{time.strftime('%H:%M:%S')} {self.command} {self.path} -> {code} (request body {length} B)\n")

    def do_GET(self):
        if self.path.startswith("/__harness-result?"):
            from urllib.parse import parse_qs, urlparse
            result = parse_qs(urlparse(self.path).query).get("r", [""])[0]
            LOG.write(f"{time.strftime('%H:%M:%S')} HARNESS RESULT: {result}\n")
            self.send_response(204)
            self.end_headers()
            return
        super().do_GET()

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def do_POST(self):  # nothing should ever POST; log and refuse
        self.send_response(405)
        self.end_headers()

    do_PUT = do_POST


http.server.ThreadingHTTPServer(("127.0.0.1", 8765), Handler).serve_forever()
