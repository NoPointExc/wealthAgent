#!/usr/bin/env python3
"""Tiny local TCP forwarder: http://localhost:<listen-port> -> your LAN server.

Why: Google sign-in only allows localhost (or public-domain) origins, so a
WealthAgent instance on a LAN IP can't show the Google button directly. Run
this on your laptop and browse http://localhost:5173 instead. Useful when the
server's sshd disallows `ssh -L` port forwarding (common on NAS appliances).

Usage:
    python3 ops/nas-proxy.py <server-ip> [server-port] [listen-port]
    python3 ops/nas-proxy.py 192.168.68.50 18080 5173
"""
import socket, sys, threading

if len(sys.argv) < 2:
    sys.exit(__doc__)
TARGET = (sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 18080)
LISTEN = ("127.0.0.1", int(sys.argv[3]) if len(sys.argv) > 3 else 5173)

def pump(src, dst):
    try:
        while (data := src.recv(65536)):
            dst.sendall(data)
    except OSError:
        pass
    finally:
        for s in (src, dst):
            try: s.shutdown(socket.SHUT_RDWR)
            except OSError: pass

def handle(client):
    try:
        upstream = socket.create_connection(TARGET, timeout=10)
    except OSError as e:
        print(f"cannot reach {TARGET}: {e}")
        client.close()
        return
    threading.Thread(target=pump, args=(client, upstream), daemon=True).start()
    threading.Thread(target=pump, args=(upstream, client), daemon=True).start()

srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(LISTEN)
srv.listen(64)
print(f"proxying http://localhost:{LISTEN[1]} -> {TARGET[0]}:{TARGET[1]}  (Ctrl-C to stop)")
try:
    while True:
        conn, _ = srv.accept()
        handle(conn)
except KeyboardInterrupt:
    pass
