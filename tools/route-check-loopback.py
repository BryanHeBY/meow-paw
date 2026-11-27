"""Run the Paw client against a trusted Meow binary in a private Linux network namespace."""
import os
import pathlib
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

def take(sock, n):
    data = b''
    while len(data) < n:
        block = sock.recv(n-len(data))
        if not block:
            raise RuntimeError('SOCKS5 connection closed early')
        data += block
    return data

def listen():
    s = socket.socket()
    s.bind(('127.0.0.1', 0))
    s.listen()
    return s

if len(sys.argv) != 3:
    sys.exit("usage: route-check-loopback.py <trusted-meow-bin> <paw-lib-test-bin>")
if {name for _, name in socket.if_nameindex()} != {"lo"}:
    sys.exit("this test requires a private loopback-only network namespace")

core = pathlib.Path(sys.argv[1]).resolve(strict=True)
paw = pathlib.Path(sys.argv[2]).resolve(strict=True)
with tempfile.TemporaryDirectory(prefix='paw-core-contract-') as d, listen() as proxy:
    with listen() as addr:
        local = addr.getsockname()[1]
    d = pathlib.Path(d)
    cfg = d/'config.yaml'
    cfg.write_text(f"""external-controller: 127.0.0.1:{local}
secret: paw-route-fixture
mode: rule
dns:
  enable: false
proxies:
  - name: selected
    type: socks5
    server: 127.0.0.1
    port: {proxy.getsockname()[1]}
rules:
  - MATCH,DIRECT
""")
    seen = []
    errors = []
    proxy.settimeout(.15)
    stop = threading.Event()
    def receive():
        while not stop.is_set():
            try:
                c,_ = proxy.accept()
            except TimeoutError:
                continue
            try:
                with c:
                    c.settimeout(2)
                    assert take(c,1) == b'\x05'
                    assert b'\x00' in take(c,take(c,1)[0])
                    c.sendall(b'\x05\x00')
                    assert take(c,4) == b'\x05\x01\x00\x03'
                    host = take(c,take(c,1)[0]).decode('ascii')
                    port = int.from_bytes(take(c,2),'big')
                    seen.append((host,port))
                    c.sendall(b'\x05\x05\x00\x01'+b'\x00'*6)
            except BaseException as e:
                errors.append(repr(e))
    t = threading.Thread(target=receive, daemon=True)
    t.start()
    url = f'http://127.0.0.1:{local}/'
    env = dict(os.environ, HOME=str(d), XDG_CONFIG_HOME=str(d),
               PAW_TEST_ROUTE_CONTROLLER=url)
    with (d/'meow.log').open('wb') as out:
        proc = subprocess.Popen([core,'-d',d,'-f',cfg], env=env, cwd=d,
                                stdin=subprocess.DEVNULL, stdout=out, stderr=out)
        try:
            ctl = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            for _ in range(60):
                if proc.poll() is not None:
                    raise RuntimeError((d/'meow.log').read_text())
                try:
                    ctl.open(urllib.request.Request(url+'version',headers={'Authorization':'Bearer paw-route-fixture'}),timeout=.3).close()
                    break
                except OSError:
                    time.sleep(.05)
            else:
                raise RuntimeError('control socket did not become ready')
            subprocess.run([paw,'real_meow_controller_route_check_contract','--ignored','--nocapture'],
                           env=env, cwd=d, timeout=15, check=True)
            time.sleep(.2)
            assert not errors, errors
            assert seen == [('www.gstatic.com', 443)], seen
            print('Live Paw + Meow: exact SOCKS5 witness',seen)
        finally:
            stop.set()
            t.join(3)
            if proc.poll() is None:
                proc.terminate()
                try: proc.wait(3)
                except subprocess.TimeoutExpired: proc.kill(); proc.wait(3)
