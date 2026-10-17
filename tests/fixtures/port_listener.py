"""Stands in for a database: listens on the port in argv and records what it was given."""
import json
import os
import socket
import sys
import time

with socket.socket() as server:
    server.bind(('127.0.0.1', int(sys.argv[1])))
    server.listen()
    seen = {key: value for key, value in os.environ.items() if key.startswith('PARCELA_PORT_')}
    record = os.path.join(os.environ['PARCELA_RUNTIME_DIR'], 'port-listener.json')
    with open(record, 'w') as handle:
        json.dump({'argv': sys.argv[1:], 'environment': seen, 'generation': os.environ['PARCELA_GENERATION']}, handle)
    while True:
        time.sleep(60)
