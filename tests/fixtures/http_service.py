import base64
import hashlib
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/ready' and os.path.exists(os.path.join(os.environ['PARCELA_RUNTIME_DIR'], 'unhealthy')):
            self.send_response(503)
            self.send_header('Content-Length', '0')
            self.end_headers()
            return
        if self.path == '/ws':
            key = self.headers['Sec-WebSocket-Key']
            accept = base64.b64encode(hashlib.sha1((key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
            self.send_response(101)
            self.send_header('Upgrade', 'websocket')
            self.send_header('Connection', 'Upgrade')
            self.send_header('Sec-WebSocket-Accept', accept)
            self.end_headers()
            self.wfile.write(b'\x81\x05hello')
            self.wfile.flush()
            return
        if self.path == '/sse':
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            self.wfile.write(b'data: ready\n\n')
            self.wfile.flush()
            return
        body = json.dumps({'id': os.environ['PARCELA_ENVIRONMENT_ID'], 'generation': os.environ['PARCELA_GENERATION'],
                           'url': os.environ['PARCELA_URL'], 'argv': sys.argv[1:], 'headers': dict(self.headers)}).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *_):
        pass

ThreadingHTTPServer((os.environ['HOST'], int(os.environ['PORT'])), Handler).serve_forever()
