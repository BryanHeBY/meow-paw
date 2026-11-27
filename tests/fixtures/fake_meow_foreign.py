#!/usr/bin/python3
"""Own the authenticated fake controller, but intentionally not the mixed port."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import sys

if "-t" in sys.argv:
    sys.exit(0)

config = Path(sys.argv[sys.argv.index("-f") + 1]).read_text()
fields = {}
for line in config.splitlines():
    if line and not line[0].isspace():
        key, _, value = line.partition(":")
        fields[key] = value.strip().strip("\"'")
host, port = fields["external-controller"].rsplit(":", 1)
assert host == "127.0.0.1"
marker = Path(os.environ["PAW_TEST_FOREIGN_MARKER"])
marker.write_text(str(int(fields["mixed-port"])))


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.headers.get("Authorization") != f"Bearer {fields['secret']}":
            self.send_error(401)
            return
        body = json.dumps({"hello": "meow", "mode": "rule"}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


with ThreadingHTTPServer((host, int(port)), Handler) as controller:
    controller.serve_forever()
