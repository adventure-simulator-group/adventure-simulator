#!/usr/bin/env python3
"""Simple proxy server that serves static files and proxies /v1/* to SpacetimeDB."""

import http.server
import urllib.request
import urllib.error
import sys
import os

SPACETIMEDB_URL = os.environ.get("SPACETIMEDB_URL", "http://localhost:3000")

class ProxyHandler(http.server.SimpleHTTPRequestHandler):
    def do_POST(self):
        if self.path.startswith("/v1/"):
            self.proxy_to_spacetimedb()
        else:
            self.send_error(404)

    def do_GET(self):
        if self.path.startswith("/v1/"):
            self.proxy_to_spacetimedb()
        else:
            super().do_GET()

    def proxy_to_spacetimedb(self):
        target_url = SPACETIMEDB_URL + self.path

        # Read request body if present
        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length) if content_length > 0 else None

        # Build request
        req = urllib.request.Request(target_url, data=body, method=self.command)
        req.add_header("Content-Type", self.headers.get("Content-Type", "application/json"))

        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                self.send_response(resp.status)
                self.send_header("Content-Type", resp.headers.get("Content-Type", "application/json"))
                self.send_header("Access-Control-Allow-Origin", "*")
                self.end_headers()
                self.wfile.write(resp.read())
        except urllib.error.HTTPError as e:
            self.send_response(e.code)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()
            self.wfile.write(e.read())
        except Exception as e:
            self.send_error(502, str(e))

    def do_OPTIONS(self):
        # Handle CORS preflight
        self.send_response(200)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.end_headers()

if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
    os.chdir(os.path.dirname(os.path.abspath(__file__)))

    print(f"Serving on http://localhost:{port}/map.html")
    print(f"Proxying /v1/* to {SPACETIMEDB_URL}")

    with http.server.HTTPServer(("127.0.0.1", port), ProxyHandler) as httpd:
        httpd.serve_forever()
