"""Serves the private demo review page (out/review/) to your phone on the
home network. Never for publishing.

    python3 tools/review_server.py install  # keep it running (login item); prints the URL
    python3 tools/review_server.py uninstall
    python3 tools/review_server.py start    # just this session, in the background
    python3 tools/review_server.py stop
    python3 tools/review_server.py status   # the current URL, checked
    python3 tools/review_server.py url      # the current URL only

It listens on this Mac's Wi-Fi/Ethernet address only (not on all
interfaces) and follows it when it changes (another network, a new DHCP
lease), answers only clients with private network addresses (home network,
this Mac), serves only out/review/, and supports byte ranges, which iPhone
Safari needs to play videos. `install` adds a per-user LaunchAgent that
starts it at login and restarts it if it stops. Development only."""

import http.server
import ipaddress
import os
import re
import signal
import subprocess
import sys
import threading
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "out" / "review"
PID = SITE / ".server.pid"
LOG = SITE / ".server.log"
PORT = 8766
AGENT = Path.home() / "Library" / "LaunchAgents" / "dev.lightable.review.plist"
LABEL = "dev.lightable.review"


def current_address():
    """This Mac's address on the home network (Wi-Fi first), or None."""
    for interface in ("en0", "en1", "en2"):
        out = subprocess.run(["ipconfig", "getifaddr", interface], capture_output=True, text=True)
        address = out.stdout.strip()
        if address and ipaddress.ip_address(address).is_private:
            return address
    return None


def lan_address():
    return current_address() or sys.exit("No private network address found (is Wi-Fi on?)")


def log(line):
    with open(LOG, "a") as f:
        f.write(f"{time.strftime('%Y-%m-%d %H:%M:%S')} {line}\n")


def answers(address):
    """Whether the page loads from `address` (as a phone would ask)."""
    try:
        with urllib.request.urlopen(f"http://{address}:{PORT}/", timeout=3) as r:
            return r.status == 200
    except OSError:
        return False


def follow():
    """Serves on the current address, moving when it changes."""
    PID.write_text(str(os.getpid()))
    server, bound = None, None
    while True:
        address = current_address()
        if address != bound:
            if server:
                server.shutdown()
                server.server_close()
                server = None
                log(f"stopped listening on {bound}")
            if address:
                try:
                    server = http.server.ThreadingHTTPServer((address, PORT), Handler)
                    threading.Thread(target=server.serve_forever, daemon=True).start()
                    log(f"listening on http://{address}:{PORT}/")
                except OSError as e:
                    log(f"could not listen on {address}: {e}")
                    address = None
            bound = address
        time.sleep(5)


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map,
                      ".mp4": "video/mp4", ".vtt": "text/vtt", ".json": "application/json"}

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(SITE), **kwargs)

    def log_message(self, fmt, *args):
        with open(LOG, "a") as log:
            log.write(f"{time.strftime('%H:%M:%S')} {self.client_address[0]} {fmt % args}\n")

    def allowed(self):
        address = ipaddress.ip_address(self.client_address[0])
        if not (address.is_private or address.is_loopback):
            self.send_error(403)
            return False
        # Hidden files (the pid file, the log) stay private.
        if any(part.startswith(".") for part in self.path.split("?")[0].split("/") if part):
            self.send_error(404)
            return False
        return True

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        self.send_header("Accept-Ranges", "bytes")
        super().end_headers()

    def do_HEAD(self):
        if self.allowed():
            super().do_HEAD()

    def do_GET(self):
        if not self.allowed():
            return
        path = Path(self.translate_path(self.path))
        match = re.fullmatch(r"bytes=(\d*)-(\d*)", self.headers.get("Range", ""))
        if not (match and path.is_file()):
            return super().do_GET()
        size = path.stat().st_size
        start = int(match[1]) if match[1] else max(0, size - int(match[2] or 0))
        end = int(match[2]) if match[1] and match[2] else size - 1
        end = min(end, size - 1)
        if start > end:
            self.send_response(416)
            self.send_header("Content-Range", f"bytes */{size}")
            self.end_headers()
            return
        self.send_response(206)
        self.send_header("Content-Type", self.guess_type(str(path)))
        self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        self.send_header("Content-Length", str(end - start + 1))
        self.end_headers()
        with open(path, "rb") as f:
            f.seek(start)
            remaining = end - start + 1
            while remaining:
                chunk = f.read(min(1 << 20, remaining))
                if not chunk:
                    break
                self.wfile.write(chunk)
                remaining -= len(chunk)

    def do_POST(self):
        self.send_error(405)

    do_PUT = do_DELETE = do_POST


def running():
    try:
        pid = int(PID.read_text())
        os.kill(pid, 0)
        return pid
    except (FileNotFoundError, ValueError, ProcessLookupError, PermissionError):
        return None


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else "status"
    SITE.mkdir(parents=True, exist_ok=True)
    if command == "serve":
        follow()
    elif command == "url":
        address = current_address()
        print(f"http://{address}:{PORT}/" if address else "no network address")
    elif command == "install":
        stop_old()
        AGENT.parent.mkdir(parents=True, exist_ok=True)
        AGENT.write_text(f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{sys.executable}</string><string>{Path(__file__).resolve()}</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>WorkingDirectory</key><string>{ROOT}</string>
  <key>StandardErrorPath</key><string>{LOG}</string>
</dict>
</plist>
""")
        domain = f"gui/{os.getuid()}"
        subprocess.run(["launchctl", "bootout", domain, str(AGENT)], capture_output=True)
        subprocess.run(["launchctl", "bootstrap", domain, str(AGENT)], check=True)
        report(wait=True)
    elif command == "uninstall":
        subprocess.run(["launchctl", "bootout", f"gui/{os.getuid()}", str(AGENT)], capture_output=True)
        AGENT.unlink(missing_ok=True)
        PID.unlink(missing_ok=True)
        print("Uninstalled.")
    elif command == "start":
        if running():
            report()
            return
        proc = subprocess.Popen([sys.executable, __file__, "serve"], start_new_session=True,
                                stdout=subprocess.DEVNULL, stderr=open(LOG, "a"))
        time.sleep(0.5)
        if proc.poll() is not None:
            sys.exit(f"The server did not start; see {LOG}")
        report(wait=True)
    elif command == "stop":
        if AGENT.exists():
            sys.exit("Kept running by its login item: use `uninstall` to stop it.")
        pid = running()
        if pid:
            os.kill(pid, signal.SIGTERM)
            print("Stopped.")
        else:
            print("Not running.")
        PID.unlink(missing_ok=True)
    else:
        report()


def stop_old():
    """Stops a server started by hand (`start`), before the login item."""
    pid = running()
    if pid:
        os.kill(pid, signal.SIGTERM)
        time.sleep(0.5)


def report(wait=False):
    """The current URL, and whether it really loads from there."""
    address = current_address()
    if not address:
        print("No private network address (is Wi-Fi on?)")
        return
    url = f"http://{address}:{PORT}/"
    ok = answers(address)
    for _ in range(20 if wait and not ok else 0):
        time.sleep(0.5)
        if ok := answers(address):
            break
    kept = " (kept running by its login item)" if AGENT.exists() else ""
    print(f"Review page: {url} - {'loads' if ok else 'NOT answering'}{kept}")


if __name__ == "__main__":
    main()
