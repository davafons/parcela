import json
import os
import sqlite3
from http.server import BaseHTTPRequestHandler, HTTPServer


class Hello(BaseHTTPRequestHandler):
    def do_GET(self):
        database = sqlite3.connect(os.environ["DATABASE_PATH"])
        (message,) = database.execute("SELECT text FROM messages LIMIT 1").fetchone()
        body = {"message": message, "url": os.environ["PARCELA_SERVICE_API_URL"]}
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(body).encode() + b"\n")


HTTPServer(("127.0.0.1", int(os.environ["PORT"])), Hello).serve_forever()
