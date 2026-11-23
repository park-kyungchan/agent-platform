# The proxy of a look on the PC (specs/ap/pc-egress.md). Run as `python3 -I <this file> <form> <argument>...` or
# `python3 -I -c <its text> <form> <argument>...`; standard library only, syntax of Python 3.7. Forms:
#   outside <socket> <nonce> <role> <names> <ranges>   judge each request as `ap run`'s proxy does, connect out
#   inside <socket> <port>                              listen on 127.0.0.1:<port>, pass each connection on
#   check                                               answer `ap-egress ready <version>` only if it refuses
# The names and ranges come from `ap` as arguments; none is written here but the one of `check`.
import sys

MINIMUM = (3, 10)
if sys.version_info < MINIMUM:
    sys.stderr.write("ap-egress: Python 3.10 or later is required\n")
    sys.exit(3)

import ipaddress
import json
import os
import re
import select
import shutil
import signal
import socket
import tempfile
import threading
import time

ROLES = ("implementer", "fixer", "reviewer", "verifier", "writer")
PHRASES = {400: "Bad Request", 403: "Forbidden", 405: "Method Not Allowed", 502: "Bad Gateway"}
REQUEST_MAX = 8192
WITHIN = 10.0
AT_ONCE = 64
SOCKET_PATH_MAX = 107


def refuse(message):
    sys.stderr.write("ap-egress: " + message + "\n")
    sys.exit(2)


# ---- the arguments ------------------------------------------------------------------------------------------

def split_socket(path):
    if not path.startswith("/") or path.endswith("/"):
        refuse("<socket> is not an absolute path of a file name")
    directory, name = os.path.split(path)
    if name in ("", ".", "..") or len(os.fsencode(name)) > SOCKET_PATH_MAX:
        refuse("<socket> has no usable last component")
    return directory or "/", name


def parse_names(text):
    names = []
    for item in text.split(",") if text else []:
        raw = item.encode("utf-8", "surrogateescape")
        if not 1 <= len(raw) <= 255 or any(not 0x21 <= b <= 0x7E for b in raw):
            refuse("a name of <names> is not a host name")
        names.append(item.lower())
    return names


def parse_ranges(text):
    ranges = []
    for item in text.split(",") if text else []:
        if not re.fullmatch(r"[0-9a-fA-F:.]+/[0-9]{1,3}", item):
            refuse("a range of <ranges> is not a network in CIDR form")
        try:
            ranges.append(ipaddress.ip_network(item, strict=True))
        except ValueError:
            refuse("a range of <ranges> is not a network in CIDR form")
    return ranges


# ---- the record: one line on standard error, one write ------------------------------------------------------

def say(nonce, role, at, target, status, reason):
    line = {
        "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(at)),
        "role": role,
        "name": target[0] if target else None,
        "port": target[1] if target else None,
        "status": status,
        "reason": reason,
    }
    text = "ap-egress " + nonce + " " + json.dumps(line, separators=(",", ":"))
    try:
        os.write(2, b"\x00" + text.encode("ascii") + b"\n")
    except OSError:
        pass


# ---- the request --------------------------------------------------------------------------------------------

def head_end(buf):
    start = 0
    while True:
        i = buf.find(b"\n", start)
        if i < 0:
            return None
        line = buf[start:i]
        if line.endswith(b"\r"):
            line = line[:-1]
        if not line:
            return i + 1
        start = i + 1


def read_request(conn):
    """The request line and the bytes after the empty line; None when not complete in time and bounds."""
    deadline = time.monotonic() + WITHIN
    buf = b""
    while True:
        end = head_end(buf)
        if end is not None:
            if end > REQUEST_MAX:
                return None
            first = buf[:end].split(b"\n", 1)[0]
            if first.endswith(b"\r"):
                first = first[:-1]
            return first, buf[end:]
        if len(buf) >= REQUEST_MAX:
            return None
        left = deadline - time.monotonic()
        if left <= 0:
            return None
        try:
            conn.settimeout(left)
            chunk = conn.recv(4096)
        except (OSError, ValueError):
            return None
        if not chunk:
            return None
        buf += chunk


def form(line):
    """None: not of the form; (None, None): another method than CONNECT; else (host, port)."""
    words = line.split(b" ")
    if len(words) != 3:
        return None
    method, target, version = words
    if not method or not target or version not in (b"HTTP/1.1", b"HTTP/1.0"):
        return None
    if method != b"CONNECT":
        return (None, None)
    colon = target.rfind(b":")
    if colon < 0:
        return None
    host, port = target[:colon], target[colon + 1:]
    if not 1 <= len(host) <= 255 or any(not 0x21 <= b <= 0x7E for b in host):
        return None
    if not 1 <= len(port) <= 5 or not port.isdigit() or port[:1] == b"0" or int(port) > 65535:
        return None
    return (host.decode("ascii"), int(port))


def is_literal(host):
    if host.startswith("[") and host.endswith("]"):
        inner = host[1:-1]
        if "%" in inner:
            return False
        try:
            ipaddress.IPv6Address(inner)
        except ValueError:
            return False
        return True
    parts = host.split(".")
    return len(parts) == 4 and all(p and all(c in "0123456789" for c in p) for p in parts)


def listed(name, names):
    return name in names


# ---- the way out: the system's resolver, or the seam of the tests -------------------------------------------

def seam_entry(entry):
    """An entry `<address>` or `<address>:<port>` (an IPv6 address bracketed) of the seam: (address, port)."""
    entry = entry.strip()
    port = None
    if entry.startswith("["):
        close = entry.find("]")
        if close < 0:
            return None
        address, rest = entry[1:close], entry[close + 1:]
        if rest:
            if not rest.startswith(":"):
                return None
            port = rest[1:]
    elif entry.count(":") == 1:
        address, port = entry.split(":")
    else:
        address = entry
    try:
        address = ipaddress.ip_address(address)
    except ValueError:
        return None
    if port is not None:
        if not port.isdigit() or not 1 <= int(port) <= 65535:
            return None
        port = int(port)
    return address, port


def resolve(name):
    """[(address, port)] of the name; port None: 443. The seam only when AP_EGRESS_RESOLVE is set."""
    seam = os.environ.get("AP_EGRESS_RESOLVE")
    found = []
    if seam:
        try:
            with open(seam, "rb") as f:
                entries = json.loads(f.read().decode("utf-8")).get(name, [])
            for entry in entries:
                parsed = seam_entry(entry)
                if parsed is not None:
                    found.append(parsed)
        except (OSError, ValueError, AttributeError, TypeError):
            return []
        return found
    try:
        infos = socket.getaddrinfo(name, 443, type=socket.SOCK_STREAM)
    except (OSError, UnicodeError):
        return []
    for info in infos:
        try:
            address = ipaddress.ip_address(info[4][0].split("%")[0])
        except ValueError:
            continue
        if (address, None) not in found:
            found.append((address, None))
    return found


def in_ranges(address, ranges):
    if address.version == 6 and address.ipv4_mapped is not None:
        address = address.ipv4_mapped
    return any(address.version == net.version and address in net for net in ranges)


def connect(address, port):
    family = socket.AF_INET6 if address.version == 6 else socket.AF_INET
    sock = socket.socket(family, socket.SOCK_STREAM)
    return sock, (str(address), 443 if port is None else port)


def judge(host, port, names, ranges):
    """(None, upstream) when admitted; else (status, reason). No connection is opened for a refusal."""
    if port != 443:
        return 403, "port", None
    if is_literal(host):
        return 403, "literal", None
    name = host.lower()
    if name.endswith("."):
        name = name[:-1]
    if not listed(name, names):
        return 403, "not_listed", None
    found = resolve(name)
    if not found:
        return 502, "unresolved", None
    if any(in_ranges(address, ranges) for address, _ in found):
        return 403, "address", None
    deadline = time.monotonic() + WITHIN
    for address, via in found:
        left = deadline - time.monotonic()
        if left <= 0:
            break
        sock, where = connect(address, via)
        try:
            sock.settimeout(left)
            sock.connect(where)
            sock.settimeout(None)
            return 200, None, sock
        except OSError:
            sock.close()
    return 502, "unreachable", None


# ---- serving ------------------------------------------------------------------------------------------------

class Open(object):
    """The sockets being served: what the close shuts."""

    def __init__(self):
        self.lock = threading.Lock()
        self.closed = False
        self.sockets = set()

    def hold(self, sock):
        with self.lock:
            if self.closed:
                return False
            self.sockets.add(sock)
            return True

    def release(self, sock):
        with self.lock:
            self.sockets.discard(sock)

    def close_all(self):
        with self.lock:
            self.closed = True
            held = list(self.sockets)
            self.sockets.clear()
        for sock in held:
            shut(sock)


def shut(sock):
    try:
        sock.shutdown(socket.SHUT_RDWR)
    except OSError:
        pass
    try:
        sock.close()
    except OSError:
        pass


def pump(source, sink, done):
    try:
        while True:
            data = source.recv(65536)
            if not data:
                break
            sink.sendall(data)
    except OSError:
        pass
    done.set()
    shut(source)
    shut(sink)


def relay(client, upstream, rest):
    client.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
    if rest:
        upstream.sendall(rest)
    client.settimeout(None)
    done = threading.Event()
    back = threading.Thread(target=pump, args=(upstream, client, done), daemon=True)
    back.start()
    pump(client, upstream, done)
    back.join()


def serve_one(conn, at, nonce, role, names, ranges, held):
    upstream = None
    try:
        if not held.hold(conn):
            return
        conn.settimeout(None)
        read = read_request(conn)
        parsed = form(read[0]) if read else None
        target = None
        upstream = None
        if parsed is None:
            status, reason = 400, "form"
        elif parsed == (None, None):
            status, reason = 405, "method"
        else:
            target = parsed
            status, reason, upstream = judge(parsed[0], parsed[1], names, ranges)
        say(nonce, role, at, target, status, reason)
        if upstream is None:
            conn.sendall(("HTTP/1.1 %d %s\r\n\r\n" % (status, PHRASES[status])).encode("ascii"))
            return
        if not held.hold(upstream):
            return
        relay(conn, upstream, read[1])
    except OSError:
        pass
    finally:
        if upstream is not None:
            held.release(upstream)
            shut(upstream)
        held.release(conn)
        shut(conn)


def listen_at(directory, name):
    """Binds a listening Unix socket at `name` of `directory` (mode 0600): it appears under its name only
    once it listens, and a name that exists refuses."""
    try:
        os.chdir(directory)
    except OSError:
        refuse("<socket>'s directory cannot be entered")
    temporary = ".ap-egress-%d-%s" % (os.getpid(), name[:40])
    old = os.umask(0o177)
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        sock.bind(temporary)
        os.chmod(temporary, 0o600)
        sock.listen(128)
        identity = os.lstat(temporary).st_ino
        os.link(temporary, name)
    except OSError as error:
        sock.close()
        try:
            os.unlink(temporary)
        except OSError:
            pass
        os.umask(old)
        refuse("<socket> cannot be bound: %s" % (error.strerror or "error"))
    os.umask(old)
    os.unlink(temporary)
    return sock, identity


def outside(directory, name, nonce, role, names, ranges):
    stop = threading.Event()

    def on_signal(number, frame):
        stop.set()

    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGHUP, on_signal)
    listener, identity = listen_at(directory, name)
    listener.settimeout(0.1)
    turns = threading.BoundedSemaphore(AT_ONCE)
    held = Open()

    def run(conn, at):
        try:
            serve_one(conn, at, nonce, role, names, ranges, held)
        finally:
            turns.release()

    while not stop.is_set():
        if not turns.acquire(timeout=0.1):
            continue
        try:
            conn, _ = listener.accept()
        except socket.timeout:
            turns.release()
            continue
        except OSError:
            turns.release()
            time.sleep(0.01)
            continue
        threading.Thread(target=run, args=(conn, time.time()), daemon=True).start()
    listener.close()
    held.close_all()
    try:
        if os.lstat(name).st_ino == identity:
            os.unlink(name)
    except OSError:
        pass


def inside_pass(client, name):
    upstream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        upstream.connect(name)
    except OSError:
        upstream.close()
        shut(client)
        return

    def copy(source, sink):
        try:
            while True:
                data = source.recv(65536)
                if not data:
                    break
                sink.sendall(data)
            sink.shutdown(socket.SHUT_WR)
        except OSError:
            shut(source)
            shut(sink)

    back = threading.Thread(target=copy, args=(upstream, client), daemon=True)
    back.start()
    copy(client, upstream)
    back.join()
    shut(client)
    shut(upstream)


def inside(directory, name, port):
    try:
        os.chdir(directory)
    except OSError:
        refuse("<socket>'s directory cannot be entered")

    def on_signal(number, frame):
        os._exit(0)

    signal.signal(signal.SIGTERM, on_signal)
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", port))
        listener.listen(128)
    except OSError as error:
        sys.stderr.write("ap-egress: cannot listen on 127.0.0.1:%d: %s\n" % (port, error.strerror))
        sys.exit(1)
    sys.stdout.write("ready\n")
    sys.stdout.flush()
    while True:
        try:
            client, _ = listener.accept()
        except OSError:
            time.sleep(0.01)
            continue
        threading.Thread(target=inside_pass, args=(client, name), daemon=True).start()


# ---- the self-check -----------------------------------------------------------------------------------------

def check():
    nonce = os.urandom(8).hex()
    try:
        directory = tempfile.mkdtemp(prefix="ap-egress-check-", dir="/tmp")
    except OSError:
        return 1
    child = None
    ready = False
    try:
        reader, writer = os.pipe()
        child = os.fork()
        if child == 0:
            code = 1
            try:
                os.close(reader)
                os.dup2(writer, 2)
                os.close(writer)
                outside(directory, "s", nonce, "reviewer", ["api.anthropic.com"], [])
                code = 0
            finally:
                os._exit(code)
        os.close(writer)
        path = os.path.join(directory, "s")
        end = time.monotonic() + 10
        while not os.path.exists(path) and time.monotonic() < end:
            time.sleep(0.02)
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        client.settimeout(10)
        client.connect(path)
        client.sendall(b"CONNECT ap-egress-check.invalid:443 HTTP/1.1\r\n\r\n")
        answer = b""
        while len(answer) < 12:
            data = client.recv(512)
            if not data:
                break
            answer += data
        client.close()
        line = b""
        while not line.endswith(b"\n") and select.select([reader], [], [], 10)[0]:
            data = os.read(reader, 4096)
            if not data:
                break
            line += data
        os.close(reader)
        prefix = b"\x00ap-egress " + nonce.encode("ascii") + b" "
        if answer.startswith(b"HTTP/1.1 403") and line.startswith(prefix):
            record = json.loads(line[len(prefix):].decode("ascii"))
            ready = record.get("reason") == "not_listed"
    except (OSError, ValueError, AttributeError):
        ready = False
    finally:
        if child:
            stop_child(child)
        shutil.rmtree(directory, ignore_errors=True)
    if not ready:
        return 1
    sys.stdout.write("ap-egress ready %d.%d.%d\n" % tuple(sys.version_info[:3]))
    return 0


def stop_child(pid):
    try:
        os.kill(pid, signal.SIGTERM)
    except OSError:
        pass
    end = time.monotonic() + 15
    while time.monotonic() < end:
        try:
            done, _ = os.waitpid(pid, os.WNOHANG)
        except OSError:
            return
        if done:
            return
        time.sleep(0.02)
    try:
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
    except OSError:
        pass


def main(argv):
    if not argv:
        refuse("a form is needed: outside, inside or check")
    form_name, rest = argv[0], argv[1:]
    if form_name == "outside" and len(rest) == 5:
        directory, name = split_socket(rest[0])
        if not re.fullmatch(r"[0-9a-f-]{1,64}", rest[1]):
            refuse("<nonce> is not 1 to 64 characters of 0-9a-f-")
        if rest[2] not in ROLES:
            refuse("<role> is not a role")
        outside(directory, name, rest[1], rest[2], parse_names(rest[3]), parse_ranges(rest[4]))
        sys.stdout.flush()
        # Threads of connections that wait on the network end with the process, not with a join.
        os._exit(0)
    if form_name == "inside" and len(rest) == 2:
        directory, name = split_socket(rest[0])
        if not re.fullmatch(r"[0-9]{1,5}", rest[1]) or not 1 <= int(rest[1]) <= 65535:
            refuse("<port> is not a whole number from 1 to 65535")
        inside(directory, name, int(rest[1]))
        return 0
    if form_name == "check" and not rest:
        return check()
    refuse("not a form of this program")


sys.exit(main(sys.argv[1:]))
