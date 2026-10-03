"""The broker against a real kernel: needs root and /dev/uhid, so not part of `cargo test`.

Runs the broker the way systemd does (the connection as fd 3, LISTEN_PID/LISTEN_FDS; only
CAP_SYS_PTRACE left in its bounding set, no_new_privs) with a seat file that makes root the person
at the seat, then checks that the device appears with UwULock's FIDO descriptor, that reports go
both ways through its hidraw node, and that a frame that isn't a 64-byte report ends the connection
and removes the device. Then the seat goes to uid 4242 and two programs of that user connect over a
listening socket (as with `Accept=yes`): the second is turned away, and the reason names the
first's executable and pid, also with /proc mounted `hidepid=invisible` (what `ProtectProc=invisible`
gives the unit, R8 I-B) when the second argument is `invisible`. In a throw-away container (the
host's kernel makes the device; SYS_ADMIN only for the /proc remount):

    docker run --rm --device /dev/uhid --device-cgroup-rule='c *:* rmw' \\
      --cap-add SYS_PTRACE --cap-add SYS_ADMIN --security-opt apparmor=unconfined \\
      -v "$PWD":/src:ro -v <target dir>:/target <image with cargo and python3> \\
      sh -c 'CARGO_TARGET_DIR=/target cargo build -p uwulock-uhid-broker --manifest-path /src/Cargo.toml &&
        python3 /src/crates/uwulock-uhid-broker/e2e/kernel.py /target/debug/uwulock-uhid-broker invisible'
"""

import ctypes, glob, os, signal, socket, struct, sys, time

BROKER = sys.argv[1]
INVISIBLE = sys.argv[2:] == ["invisible"]
SEATS = "/run/systemd/seats"
# Not 1000: the host's own UwULock key (uwulock-1000) must not count.
USER = 4242
CAP_SYS_PTRACE = 19
PR_CAPBSET_DROP = 24
PR_SET_NO_NEW_PRIVS = 38

def seat(uid):
    os.makedirs(SEATS, exist_ok=True)
    with open(SEATS + "/seat0.new", "w") as f:
        f.write(f"IS_SEAT0=1\nACTIVE_UID={uid}\n")
    os.replace(SEATS + "/seat0.new", SEATS + "/seat0")

sys.stdout.reconfigure(line_buffering=True)
signal.alarm(60)  # a hang fails instead of waiting forever
os.makedirs("/run/uwulock", exist_ok=True)
seat(0)
if INVISIBLE:
    # What ProtectProc=invisible gives the unit: other users' processes hidden from those
    # who may not ptrace them.
    if os.system("mount -o remount,hidepid=invisible /proc") != 0:
        sys.exit("couldn't remount /proc with hidepid=invisible (needs SYS_ADMIN)")

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

def broker(connection):
    """The broker for one connection, as the unit starts it: root, CAP_SYS_PTRACE the only
    capability left in its bounding set, no_new_privs."""
    pid = os.fork()
    if pid == 0:
        libc = ctypes.CDLL(None, use_errno=True)
        last = int(open("/proc/sys/kernel/cap_last_cap").read())
        for cap in range(last + 1):
            if cap != CAP_SYS_PTRACE and libc.prctl(PR_CAPBSET_DROP, cap, 0, 0, 0) != 0:
                os._exit(90)
        if libc.prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0:
            os._exit(91)
        os.dup2(connection.fileno(), 3)
        env = dict(os.environ, LISTEN_PID=str(os.getpid()), LISTEN_FDS="1")
        os.execve(BROKER, [BROKER], env)
    connection.close()
    return pid

def start():
    parent, child = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
    return parent, broker(child)

# A program of the user: runs as USER (exec'd, like UwULock), connects to the listener, hands
# the connection over and stays until told to go.
CLIENT = """
import os, socket, sys
back = socket.socket(fileno=4)
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[1])
socket.send_fds(back, [b"visible" if os.path.exists("/proc/1") else b"hidden"], [s.fileno()])
back.recv(1)
"""

LISTENER = "/run/uwulock/e2e.sock"

def connect_as(uid, listener):
    here, there = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
    pid = os.fork()
    if pid == 0:
        os.setgroups([])
        os.setresgid(uid, uid, uid)
        os.setresuid(uid, uid, uid)
        os.dup2(there.fileno(), 4)
        os.execv(sys.executable, [sys.executable, "-c", CLIENT, LISTENER])
    there.close()
    seen, fds, _, _ = socket.recv_fds(here, 16, 1)
    # Not into the brokers started later: a broker holding its own app's end never sees it go.
    os.set_inheritable(fds[0], False)
    app = socket.socket(fileno=fds[0])
    app.settimeout(10)
    connection, _ = listener.accept()
    return app, pid, here, seen, broker(connection)

def device(uid=0):
    for d in glob.glob("/sys/bus/hid/devices/*"):
        try:
            ev = open(d + "/uevent").read()
        except OSError:
            continue
        if f"HID_UNIQ=uwulock-{uid}\n" in ev + "\n":
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

# Two at once for the same user: the second is turned away, and told which program holds the key.
seat(USER)
listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
listener.bind(LISTENER)
os.chmod(LISTENER, 0o666)
listener.listen(8)
a, client_a, keep_a, seen, pa = connect_as(USER, listener)
if INVISIBLE:
    assert seen == b"hidden", "hidepid=invisible isn't in effect"
    print("/proc hides root's processes from uid", USER)
assert recv_frame(a)[0] == 0
b, client_b, keep_b, _, pb = connect_as(USER, listener)
kind, body = recv_frame(b)
assert kind == 0x7f and b"already" in body, (kind, body)
assert b"held by /" in body and f"(pid {client_a})".encode() in body, body
print("second connection:", body)
for keep, client in ((keep_b, client_b), (keep_a, client_a)):
    keep.close()
    os.waitpid(client, 0)
a.close()
b.close()
os.waitpid(pa, 0); os.waitpid(pb, 0)
for _ in range(100):
    if not device(USER): break
    time.sleep(0.05)
assert not device(USER), "device still there after the app went"
print("device gone after the app closed the connection")
