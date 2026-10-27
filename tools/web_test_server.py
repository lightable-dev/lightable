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
        # Compress as GitHub Pages does, so load times match the real site.
        path = Path(self.translate_path(self.path))
        if (path.suffix in (".wasm", ".js", ".html", ".svg") and path.is_file()
                and "gzip" in self.headers.get("Accept-Encoding", "")):
            import gzip
            body = gzip.compress(path.read_bytes(), 6)
            self.send_response(200)
            self.send_header("Content-Type", self.guess_type(str(path)))
            self.send_header("Content-Encoding", "gzip")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        super().do_GET()

    def end_headers(self):
        # Nothing is cached, except the app's WebAssembly for a minute, so a
        # browser opening several pages (tools/demo.mjs) compiles it once.
        wasm = self.path.split("?")[0].endswith(".wasm")
        self.send_header("Cache-Control", "max-age=60" if wasm else "no-store")
        super().end_headers()

    def do_POST(self):  # nothing should ever POST; log and refuse
        self.send_response(405)
        self.end_headers()

    do_PUT = do_POST


http.server.ThreadingHTTPServer(("127.0.0.1", 8765), Handler).serve_forever()
