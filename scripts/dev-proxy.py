import socket, threading, select, sys
from urllib.parse import urlsplit
def pipe(a, b):
    try:
        while True:
            r, _, _ = select.select([a, b], [], [], 60)
            if not r: break
            for s in r:
                d = s.recv(65536)
                if not d: return
                (b if s is a else a).sendall(d)
    except Exception: pass
def handle(c):
    try:
        req = b''
        while b'\r\n\r\n' not in req:
            d = c.recv(65536)
            if not d: return
            req += d
        line = req.split(b'\r\n',1)[0].decode()
        method, target, _ = line.split(' ',2)
        sys.stderr.write(f'{method} {target}\n')
        if method == 'CONNECT':
            host, port = target.rsplit(':',1)
            up = socket.create_connection((host, int(port)), timeout=20)
            c.sendall(b'HTTP/1.1 200 Connection Established\r\n\r\n')
        else:
            u = urlsplit(target); host = u.hostname; port = u.port or 80
            up = socket.create_connection((host, port), timeout=20)
            path = u.path + ('?'+u.query if u.query else '')
            head, body = req.split(b'\r\n\r\n',1)
            lines = head.decode().split('\r\n')
            lines[0] = f'{method} {path} HTTP/1.1'
            lines = [l for l in lines if not l.lower().startswith('proxy-')]
            up.sendall(('\r\n'.join(lines)+'\r\n\r\n').encode()+body)
        pipe(c, up)
    except Exception as e:
        sys.stderr.write(f'proxy error: {e}\n')
    finally:
        try: c.close()
        except Exception: pass
srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(('127.0.0.1', 3128)); srv.listen(32)
sys.stderr.write('proxy listening on 127.0.0.1:3128\n')
while True:
    c, _ = srv.accept(); threading.Thread(target=handle, args=(c,), daemon=True).start()
