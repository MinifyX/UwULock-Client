"""The broker against a real kernel: needs root and /dev/uhid, so not part of `cargo test`.

Runs the broker the way systemd does (the connection as fd 3, LISTEN_PID/LISTEN_FDS) with a
seat file that makes root the person at the seat, then checks that the device appears with
UwULock's FIDO descriptor, that reports go both ways through its hidraw node, that a frame that
isn't a 64-byte report ends the connection and removes the device, and that a user gets one
device at a time. In a throw-away container (the host's kernel makes the device):

    cargo build -p uwulock-uhid-broker
    mkdir -p /tmp/seats && printf 'IS_SEAT0=1\\nACTIVE_UID=0\\n' > /tmp/seats/seat0
    docker run --rm --device /dev/uhid --device-cgroup-rule='c *:* rmw' \\
      -v "$PWD":/src:ro -v /tmp/seats:/run/systemd/seats:ro <image with python3> \\
      sh -c 'mkdir -p /run/uwulock && python3 /src/crates/uwulock-uhid-broker/e2e/kernel.py \\
        /src/target/debug/uwulock-uhid-broker'
"""

import os, socket, struct, sys, time, glob

BROKER = sys.argv[1]

def frame(kind, payload=b""):
    return bytes([kind]) + len(payload).to_bytes(2, "big") + payload

def recv_frame(s):
    head = b""
    while len(head) < 3:
        c = s.recv(3 - len(head))
        if not c: raise EOFError("closed")
        head += c
    n = int.from_bytes(head[1:3], "big")
    body = b""
    while len(body) < n:
        c = s.recv(n - len(body))
        if not c: raise EOFError("closed")
        body += c
    return head[0], body

def start():
    parent, child = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
    pid = os.fork()
    if pid == 0:
        os.dup2(child.fileno(), 3)
        env = dict(os.environ, LISTEN_PID=str(os.getpid()), LISTEN_FDS="1")
        os.execve(BROKER, [BROKER], env)
    child.close()
    return parent, pid

def device():
    for d in glob.glob("/sys/bus/hid/devices/*"):
        try:
            ev = open(d + "/uevent").read()
        except OSError:
            continue
        if "HID_UNIQ=uwulock-0" in ev:
            nodes = os.listdir(d + "/hidraw") if os.path.isdir(d + "/hidraw") else []
            return d, ev, nodes
    return None

app, pid = start()
app.settimeout(10)
kind, body = recv_frame(app)
assert kind == 0, (kind, body)
print("READY")
for _ in range(100):
    found = device()
    if found and found[2]: break
    time.sleep(0.05)
d, ev, nodes = found
print("device", os.path.basename(d), nodes, [l for l in ev.splitlines() if l.startswith(("HID_NAME", "HID_ID"))])
rd = open(d + "/report_descriptor", "rb").read()
assert rd[:3] == bytes([0x06, 0xd0, 0xf1]), rd
node = nodes[0]
major, minor = map(int, open(f"/sys/class/hidraw/{node}/dev").read().split(":"))
path = f"/tmp/{node}"
os.mknod(path, 0o600 | 0o020000, os.makedev(major, minor))
hid = os.open(path, os.O_RDWR)
kind, body = recv_frame(app)
assert kind == 2 and body.decode() == node, (kind, body)
print("OPEN", body)
# browser -> key: CTAPHID INIT on broadcast
report = b"\xff\xff\xff\xff\x86\x00\x08" + b"12345678"
report = report.ljust(64, b"\0")
os.write(hid, b"\0" + report)
kind, body = recv_frame(app)
assert kind == 1 and body == report, (kind, body)
print("REPORT to app ok")
# key -> browser
answer = bytes(range(64))
app.sendall(frame(1, answer))
got = os.read(hid, 64)
assert got == answer, got
print("REPORT to browser ok")
# a raw uhid DESTROY/CREATE2 smuggled as a report: refused, device gone
app.sendall(frame(1, struct.pack("=I", 11) + b"\0" * 300))
kind, body = recv_frame(app)
while kind == 3:  # the browser side closing may come first
    kind, body = recv_frame(app)
assert kind == 0x7f, (kind, body)
print("ERROR", body)
os.waitpid(pid, 0)
os.close(hid)
for _ in range(100):
    if not device(): break
    time.sleep(0.05)
assert not device(), "device still there"
print("device gone after the refused frame")

# Two at once for the same user: the second is turned away.
a, pa = start()
a.settimeout(10)
assert recv_frame(a)[0] == 0
b, pb = start()
b.settimeout(10)
kind, body = recv_frame(b)
assert kind == 0x7f and b"already" in body, (kind, body)
print("second connection:", body)
a.close()
os.waitpid(pa, 0); os.waitpid(pb, 0)
for _ in range(100):
    if not device(): break
    time.sleep(0.05)
assert not device(), "device still there after the app went"
print("device gone after the app closed the connection")
