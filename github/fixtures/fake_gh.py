"""A fake `gh` for the tests of github/apply: invented repositories only, no network.

It understands `gh api [--method M] ENDPOINT [--input FILE|-]`, answers in the real gh's forms (a success is
exit 0 with the body on stdout, a 204 with empty stdout; a failure is exit 1 with the JSON body on stdout and
`gh: <message> (HTTP <status>)` on stderr) and keeps its state in the JSON file FAKE_GH_STATE. Each call is
appended to FAKE_GH_LOG as one JSON line: its arguments, its body, and the files of FAKE_GH_UNDO_DIR at the
time of the call. Faults, in the state's "faults": "fail_write" (the k-th call that is not a GET answers 422),
"fail_get" (a GET whose path ends with a key answers that key's status), "ignore_write" (a write whose path
ends with one of these answers success and changes nothing), "strict_delete" (a DELETE of a feature that is
off answers 422).
"""
import json
import os
import re
import sys

MARKER = "fake-gh-of-the-github-apply-tests"


class Reply(Exception):
    def __init__(self, status, body=None, message=None):
        super().__init__(message)
        self.status, self.body, self.message = status, body, message


def fail(status, message):
    raise Reply(status, message=message)


def ruleset_summary(repo, ruleset):
    return {"id": ruleset["id"], "name": ruleset["name"], "target": ruleset["target"], "source_type": "Repository",
            "source": repo, "enforcement": ruleset["enforcement"]}


def ruleset_full(repo, ruleset):
    full = dict(ruleset)
    full.update({"source_type": "Repository", "source": repo, "current_user_can_bypass": "never",
                 "node_id": f"fixture-{ruleset['id']}", "created_at": "2026-01-01T00:00:00Z"})
    return full


def repository_answer(name, repo):
    answer = {"full_name": name, "private": repo["private"]}
    answer.update(repo["settings"])
    analysis = repo["security_and_analysis"]
    answer["security_and_analysis"] = None if analysis is None else {k: {"status": v} for k, v in analysis.items()}
    if repo["private"]:
        answer["temp_clone_token"] = "fixture-value-that-present-json-must-not-hold"
    return answer


def handle(state, method, path, body):
    found = re.match(r"^repos/([^/]+/[^/]+)(/.*)?$", path)
    if not found or found.group(1) not in state["repositories"]:
        fail(404, "Not Found")
    name, rest = found.group(1), found.group(2) or ""
    repo = state["repositories"][name]
    faults = state.get("faults", {})
    if method == "GET":
        for suffix, status in faults.get("fail_get", {}).items():
            if path.endswith(suffix):
                fail(status, "Fault planted by the test")
    else:
        state["writes"] = state.get("writes", 0) + 1
        if state["writes"] == faults.get("fail_write"):
            fail(422, "Validation Failed")
        if any(path.endswith(s) for s in faults.get("ignore_write", [])):
            raise Reply(204)
    if rest == "":
        if method == "GET":
            raise Reply(200, repository_answer(name, repo))
        if method == "PATCH":
            for key, value in body.items():
                if key in repo["settings"]:
                    repo["settings"][key] = value
                elif key == "security_and_analysis" and repo["security_and_analysis"] is not None:
                    for feature, item in value.items():
                        repo["security_and_analysis"][feature] = item["status"]
                else:
                    fail(422, f"Unknown member {key}")
            raise Reply(200, repository_answer(name, repo))
    elif rest == "/actions/permissions":
        if method == "GET":
            raise Reply(200, repo["actions"])
        if method == "PUT":
            if not set(body) <= {"enabled", "allowed_actions", "sha_pinning_required"} or "enabled" not in body:
                fail(422, "Invalid request")
            repo["actions"].update(body)
            raise Reply(204)
    elif rest == "/actions/permissions/selected-actions":
        if repo["actions"]["allowed_actions"] != "selected":
            fail(409, "Conflict: allowed_actions is not selected")
        if method == "GET":
            raise Reply(200, repo["selected_actions"])
        if method == "PUT":
            repo["selected_actions"] = body
            raise Reply(204)
    elif rest in ("/vulnerability-alerts", "/automated-security-fixes"):
        key = rest[1:].replace("-", "_")
        if method == "GET":
            if key == "automated_security_fixes":
                raise Reply(200, {"enabled": repo[key], "paused": False})
            if repo[key]:
                raise Reply(204)
            fail(404, "Vulnerability alerts are disabled.")
        if method == "PUT":
            if key == "automated_security_fixes" and not repo["vulnerability_alerts"]:
                fail(422, "Vulnerability alerts are not enabled")
            repo[key] = True
            raise Reply(204)
        if method == "DELETE":
            if faults.get("strict_delete") and not repo[key]:
                fail(422, "Already disabled")
            repo[key] = False
            raise Reply(204)
    elif rest == "/branches/main/protection":
        if method == "GET":
            if repo["classic_protection"]:
                raise Reply(200, {"url": f"https://api.github.com/repos/{name}/branches/main/protection"})
            fail(404, "Branch not protected")
    elif rest == "/rulesets":
        if method == "GET":
            raise Reply(200, [ruleset_summary(name, r) for r in repo["rulesets"]])
        if method == "POST":
            created = dict(body, id=state["next_ruleset_id"])
            state["next_ruleset_id"] += 1
            repo["rulesets"].append(created)
            raise Reply(201, ruleset_full(name, created))
    else:
        found = re.match(r"^/rulesets/(\d+)$", rest)
        match = [r for r in repo["rulesets"] if found and r["id"] == int(found.group(1))]
        if not match:
            fail(404, "Not Found")
        if method == "GET":
            raise Reply(200, ruleset_full(name, match[0]))
        if method == "PUT":
            match[0].update(body)
            raise Reply(200, ruleset_full(name, match[0]))
        if method == "DELETE":
            repo["rulesets"].remove(match[0])
            raise Reply(204)
    fail(404, "Not Found")


def main(argv):
    if argv[:1] != ["api"]:
        sys.stderr.write(f"unknown command {argv[:1]!r} for the fake gh\n")
        return 1
    method, endpoint, source = None, None, None
    i = 1
    while i < len(argv):
        if argv[i] in ("--method", "-X") and i + 1 < len(argv):
            method, i = argv[i + 1], i + 2
        elif argv[i] == "--input" and i + 1 < len(argv):
            source, i = argv[i + 1], i + 2
        elif argv[i].startswith("-") or endpoint is not None:
            sys.stderr.write(f"the fake gh does not take {argv[i]!r}\n")
            return 1
        else:
            endpoint, i = argv[i], i + 1
    if endpoint is None:
        sys.stderr.write("the fake gh needs an endpoint\n")
        return 1
    body = None
    if source is not None:
        body = json.load(sys.stdin if source == "-" else open(source, encoding="utf-8"))
    method = method or ("POST" if source is not None else "GET")
    undo_dir = os.environ.get("FAKE_GH_UNDO_DIR")
    undo_files = sorted(os.listdir(undo_dir)) if undo_dir and os.path.isdir(undo_dir) else None
    with open(os.environ["FAKE_GH_LOG"], "a", encoding="utf-8") as log:
        log.write(json.dumps({"argv": argv, "body": body, "undo_files": undo_files}) + "\n")
    with open(os.environ["FAKE_GH_STATE"], encoding="utf-8") as source_file:
        state = json.load(source_file)
    try:
        handle(state, method, endpoint.lstrip("/").split("?")[0], body)
        reply = Reply(500, message="the fake gh gave no answer")
    except Reply as answer:
        reply = answer
    with open(os.environ["FAKE_GH_STATE"], "w", encoding="utf-8") as state_file:
        json.dump(state, state_file, indent=1)
    if reply.message is not None:
        sys.stdout.write(json.dumps({"message": reply.message, "documentation_url": "https://docs.github.com/rest",
                                     "status": str(reply.status)}))
        sys.stderr.write(f"gh: {reply.message} (HTTP {reply.status})\n")
        return 1
    if reply.body is not None:
        sys.stdout.write(json.dumps(reply.body))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
