"""What the batch jobs share: the Academy rule, the key of a request, the request and result lines, the reading of an
answer against a verdict schema, the calls of the store engine and the pass that keeps one detector's findings.

Each job's programs load this file by its path. The standard library only; no socket, no model. Contract:
evals/batch/README.md.
"""
import hashlib
import json
import os
import stat
import subprocess
import sys
import time
import unicodedata

# The namespace of every object id of the host's store: the one of id() in crates/ap/src/host.rs.
NAMESPACE = {"instance": "vps://palantirkc", "account": "root"}
RESULT_TYPES = ("succeeded", "errored", "canceled", "expired")
FAILED_TYPES = ("errored", "canceled", "expired")
CAP = 10  # open_finding and reopen_finding answered applied, together, per call
TIMEOUT = 60  # seconds per call of the engine
BUSY_RETRIES = 5


class Refuse(Exception):
    """A refusal: the code and the members of the answer, and the exit status."""

    def __init__(self, code, exit_code=2, **members):
        super().__init__(code)
        self.answer = {"status": code, **members}
        self.exit_code = exit_code


def emit(answer):
    sys.stdout.write(json.dumps(answer, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def main(run):
    """Runs `run(argv)`, which returns (answer, exit); a Refuse or any exception is one line, exit 2."""
    try:
        answer, code = run(sys.argv[1:])
    except Refuse as r:
        answer, code = r.answer, r.exit_code
    except Exception:  # noqa: BLE001 - every other failure is internal_error, standard error stays empty
        answer, code = {"status": "internal_error"}, 2
    emit(answer)
    return code


# The Academy rule.
def is_academy(path):
    """A path is the Academy's when a component begins with `education-`, `edu-` or `academy-`, or a component
    `evals` is directly followed by a component `lesson`."""
    parts = [p for p in str(path).split("/") if p]
    for i, part in enumerate(parts):
        if part.startswith(("education-", "edu-", "academy-")):
            return True
        if part == "evals" and i + 1 < len(parts) and parts[i + 1] == "lesson":
            return True
    return False


def refuse_academy(path):
    if is_academy(path):
        raise Refuse("academy_path")


def resolve(path):
    """The absolute path that `path` names, resolved one component at a time: the rule is asked of the string as
    given, of the path so far before each lstat or readlink of it, and of a link's target before it is followed.
    A component that does not exist ends the walk; the rest is joined as given. Refuses `academy_path`."""
    path = str(path)
    refuse_academy(path)
    if path.startswith("/"):
        current = "/"
    else:
        current = os.getcwd()
        refuse_academy(current)
    pending = [p for p in path.split("/")][::-1]
    links = 0
    while pending:
        part = pending.pop()
        if part in ("", "."):
            continue
        if part == "..":
            current = os.path.dirname(current) or "/"
            continue
        candidate = os.path.join(current, part)
        refuse_academy(candidate)
        try:
            st = os.lstat(candidate)
        except OSError:
            rest = os.path.join(candidate, *[p for p in pending[::-1] if p])
            refuse_academy(rest)
            return os.path.normpath(rest)
        if not stat.S_ISLNK(st.st_mode):
            current = candidate
            continue
        target = os.readlink(candidate)
        refuse_academy(target)
        refuse_academy(os.path.join(current, target))
        links += 1
        if links > 40:
            return candidate  # a loop: the caller's open fails
        if target.startswith("/"):
            current = "/"
        pending.extend([p for p in target.split("/")][::-1])
    return current


# Keys and lines.
def canon(value):
    """JSON with sorted keys, no white space between tokens and non-ASCII characters unescaped."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def sha256(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def key(params):
    """The 64 lower-case hex digits of the SHA-256 of `params` in canonical JSON, in UTF-8."""
    return sha256(canon(params))


def request_line(params):
    """One entry of `requests` of the Message Batches API, as one line."""
    return canon({"custom_id": key(params), "params": params})


def result_line(line):
    """(custom_id, result) of a result line, or None when the line is no result line."""
    try:
        v = json.loads(line)
    except ValueError:
        return None
    if not isinstance(v, dict) or not isinstance(v.get("custom_id"), str):
        return None
    result = v.get("result")
    if not isinstance(result, dict) or result.get("type") not in RESULT_TYPES:
        return None
    if result["type"] == "succeeded" and not isinstance(result.get("message"), dict):
        return None
    return v["custom_id"], result


def result_lines(path):
    """The lines of a result file that are not empty, each as (custom_id, result) or None. Refuses
    `results_unreadable` when the file cannot be read."""
    try:
        with open(path, "rb") as f:
            data = f.read()
    except OSError:
        raise Refuse("results_unreadable")
    out = []
    for raw in data.split(b"\n"):
        if not raw.strip():
            continue
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            out.append(None)
            continue
        out.append(result_line(text))
    return out


def valid(value, schema):
    """Whether `value` holds the JSON schema `schema`, of the subset a verdict schema uses: type object, array,
    string, integer or boolean; enum; properties, required and additionalProperties false; items."""
    t = schema.get("type")
    if t == "object":
        if not isinstance(value, dict):
            return False
        props = schema.get("properties", {})
        if any(k not in value for k in schema.get("required", [])):
            return False
        if schema.get("additionalProperties") is False and any(k not in props for k in value):
            return False
        return all(valid(v, props[k]) for k, v in value.items() if k in props)
    if t == "array":
        return isinstance(value, list) and all(valid(v, schema.get("items", {})) for v in value)
    if t == "string" and not isinstance(value, str):
        return False
    if t == "integer" and (not isinstance(value, int) or isinstance(value, bool)):
        return False
    if t == "boolean" and not isinstance(value, bool):
        return False
    if "enum" in schema and value not in schema["enum"]:
        return False
    return True


def answer(result, schema):
    """How a counted result is read: ("errored"|"refused"|"unreadable", None) or ("read", the verdict)."""
    if result["type"] in FAILED_TYPES:
        return "errored", None
    message = result["message"]
    stop = message.get("stop_reason")
    if stop == "refusal":
        return "refused", None
    if stop != "end_turn":
        return "unreadable", None
    content = message.get("content")
    if not isinstance(content, list):
        return "unreadable", None
    text = "".join(b["text"] for b in content
                   if isinstance(b, dict) and b.get("type") == "text" and isinstance(b.get("text"), str))
    try:
        verdict = json.loads(text)
    except ValueError:
        return "unreadable", None
    if not valid(verdict, schema):
        return "unreadable", None
    return "read", verdict


def plain(text):
    """Each control character a space."""
    return "".join(" " if unicodedata.category(c) == "Cc" else c for c in text)


def cut(text, limit):
    """At most `limit` bytes of UTF-8, cut on a character boundary."""
    return text.encode("utf-8")[:limit].decode("utf-8", "ignore")


def write_files(out, files):
    """Writes each (name, bytes) into the directory `out` (made with mode 0700 when absent), each under a
    temporary name and renamed, mode 0600. Refuses `out_unwritable`."""
    temps = []
    try:
        if not os.path.isdir(out):
            os.mkdir(out, 0o700)
            os.chmod(out, 0o700)
        for name, data in files:
            tmp = os.path.join(out, f".{name}.tmp-{os.getpid()}")
            fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            temps.append(tmp)
            with os.fdopen(fd, "wb") as f:
                f.write(data)
            os.chmod(tmp, 0o600)
        for (name, _), tmp in zip(files, temps):
            os.replace(tmp, os.path.join(out, name))
    except OSError:
        for tmp in temps:
            try:
                os.unlink(tmp)
            except OSError:
                pass
        raise Refuse("out_unwritable")


# The store engine.
def finding_id(native_key):
    return {"object_type": "finding", "namespace": NAMESPACE, "native_key": native_key}


class Engine:
    """`ont` (ONT_BIN), with `--store` when given; each call one JSON request on standard input."""

    def __init__(self, store, client, role=None):
        self.bin = os.environ.get("ONT_BIN", "ont")
        # `role` is the one a caller of this program names in the body of each apply (specs/ap/roles.md R3): a fixed
        # member, since the environment's ONT_ROLE is dropped below.
        self.store, self.client, self.role = store, client, role
        # The action id of a request id depends on the client and the agent: neither is taken from the environment.
        self.env = {k: v for k, v in os.environ.items() if k not in ("ONT_AGENT", "ONT_ROLE")}

    def call(self, args, body):
        if self.role and args[:2] == ["action", "apply"] and "role" not in body:
            body = {**body, "role": self.role}
        command = [self.bin] + (["--store", self.store] if self.store else []) + args
        for attempt in range(BUSY_RETRIES + 1):
            try:
                done = subprocess.run(command, input=json.dumps(body), capture_output=True, text=True,
                                      timeout=TIMEOUT, env=self.env)
                v = json.loads(done.stdout)
            except (OSError, ValueError, subprocess.TimeoutExpired):
                return None, False
            if isinstance(v, dict) and v.get("code") == "store_busy" and attempt < BUSY_RETRIES:
                time.sleep(0.2)
                continue
            return (v if isinstance(v, dict) else None), done.returncode == 0

    def search(self, object_type, where):
        """Every object of every page as (native_key, fields), each field its value or None; None when an answer
        is not `ok` and qualified."""
        body = {"object_type": object_type, "where": {"all": where}, "limit": 15}
        found = []
        while True:
            v, ok = self.call(["search"], body)
            if not ok or v is None or v.get("status") != "ok" or not isinstance(v.get("result"), dict):
                return None
            r = v["result"]
            if not isinstance(r.get("basis"), dict) or r["basis"].get("qualified") is not True:
                return None
            if not isinstance(r.get("objects"), list):
                return None
            for o in r["objects"]:
                nk = o.get("id", {}).get("native_key") if isinstance(o, dict) else None
                fields = o.get("head", {}).get("fields") if isinstance(o, dict) else None
                if not isinstance(nk, str) or not isinstance(fields, dict):
                    return None
                found.append((nk, {k: (f.get("value") if isinstance(f, dict) and f.get("kind") == "value" else None)
                                   for k, f in fields.items()}))
            c = r.get("continuation")
            if c is None:
                return found
            if c == body.get("continuation"):
                return None
            body["continuation"] = c

    def apply(self, body):
        """("applied"|"already_applied", None) or ("failed", code)."""
        v, ok = self.call(["action", "apply", "--client", self.client], body)
        if v is None:
            return "failed", "store_answer_unreadable"
        if v.get("status") == "refused" and isinstance(v.get("code"), str):
            failed = v.get("failed")
            first = failed[0] if isinstance(failed, list) and failed and isinstance(failed[0], str) else None
            if v["code"] == "submission_criteria" and first:
                return "failed", first
            return "failed", v["code"]
        outcome = v.get("result", {}).get("outcome") if isinstance(v.get("result"), dict) else None
        if ok and v.get("status") == "ok" and outcome in ("applied", "already_applied"):
            return outcome, None
        return "failed", "store_answer_unreadable"


class Pass:
    """The finding pass of one detector, as `ap watch` keeps a finding: per signal by its finding's state, closes,
    the cap of ten opens and reopens answered applied, and request ids from the content of each action."""

    def __init__(self, engine, detector, agent, prefix):
        self.engine, self.detector, self.agent, self.prefix = engine, detector, agent, prefix
        self.found = {}
        self.applied_opens = 0
        self.lists = {"opened": [], "seen": [], "reopened": [], "closed": []}
        self.counts = {"dismissed": 0, "over_cap": 0, "already_applied": 0}
        self.failed = []

    def load(self):
        found = self.engine.search("finding", [{"eq": ["detector", self.detector]}])
        if found is None:
            raise Refuse("store_failed", exit_code=3)
        self.found = {k: f for k, f in found if f.get("detector") == self.detector}

    def request_id(self, action, native_key, basis, parameters):
        return self.prefix + sha256(canon([action, native_key, basis, parameters]))[:40]

    def apply(self, action, native_key, basis, parameters, done):
        capped = action in ("open_finding", "reopen_finding")
        if capped and self.applied_opens >= CAP:
            self.counts["over_cap"] += 1
            return
        body = {"action_type": action, "parameters": parameters, "agent": self.agent,
                "request_id": self.request_id(action, native_key, basis, parameters)}
        outcome, code = self.engine.apply(body)
        if outcome == "applied":
            self.lists[done].append(native_key)
            if capped:
                self.applied_opens += 1
        elif outcome == "already_applied":
            self.counts["already_applied"] += 1
        else:
            self.failed.append({"key": native_key, "code": code})

    def signal(self, native_key, basis, fields):
        """One signal: `fields` holds kind, summary, evidence and severity."""
        f = self.found.get(native_key)
        fid = finding_id(native_key)
        state = f.get("state") if f else None
        if f is None:
            p = {"finding": fid, "kind": fields["kind"], "detector": self.detector, "summary": fields["summary"],
                 "evidence": fields["evidence"], "severity": fields["severity"]}
            self.apply("open_finding", native_key, basis, p, "opened")
        elif state in ("open", "planned"):
            if f.get("evidence") != fields["evidence"]:
                self.apply("see_finding_again", native_key, basis, {"finding": fid, "evidence": fields["evidence"]},
                           "seen")
        elif state == "closed":
            self.apply("reopen_finding", native_key, basis, {"finding": fid, "evidence": fields["evidence"]},
                       "reopened")
        elif state == "dismissed":
            self.counts["dismissed"] += 1

    def live(self, native_key):
        f = self.found.get(native_key)
        return bool(f) and f.get("state") in ("open", "planned")

    def close(self, native_key, basis, reason):
        if self.live(native_key):
            p = {"finding": finding_id(native_key), "reason": cut(reason, 600)}
            self.apply("close_finding", native_key, basis, p, "closed")

    def answer(self):
        return {**self.lists, **self.counts, "failed": self.failed}
