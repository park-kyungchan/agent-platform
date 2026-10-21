#!/usr/bin/env python3
"""Thin finite native adapter; upstream jevkit owns transport and ranking."""
from __future__ import annotations

import contextlib
import hashlib
import io
import json
import math
import os
from pathlib import Path
import re
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
MODEL = "typesafe-ai/jev-1.14.0"
MODELS_SEEN = {MODEL, "jev-1.14.0"}
BASE = "https://ai-gateway.vercel.sh/typesafe"
ENDPOINT = BASE + "/v1/systemone"
HEX = re.compile(r"[0-9a-f]{64}\Z")
LABEL = re.compile(r"[A-Za-z0-9_.:-]{1,96}\Z")
WORKER_EVENTS = {"phase_changed", "tool_started", "tool_completed", "output_changed", "input_requested", "native_exit"}
WORKER_LIVENESS = {"reasoning", "tool_running", "awaiting_input", "idle", "exited", "unknown"}
WORKER_ACTIONS = {"keep_waiting", "answer_question", "nudge", "escalate", "collect"}


class Refusal(Exception):
    pass


def require(ok: bool, code: str) -> None:
    if not ok:
        raise Refusal(code)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def number(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def verify_sources(manifest: dict) -> None:
    require(digest(Path(__file__).read_bytes()) == manifest["adapterSha256"], "adapter_changed")
    for entry in manifest["upstreamFiles"]:
        rel = Path(entry["relativePath"])
        require(not rel.is_absolute() and ".." not in rel.parts, "invalid_source_pin")
        path = HERE / rel
        require(path.is_file() and not path.is_symlink(), "source_missing")
        require(digest(path.read_bytes()) == entry["sha256"], "upstream_changed")
    for entry in manifest["installedNativeSourcePins"]:
        require(digest(Path(entry["path"]).read_bytes()) == entry["sha256"], "native_source_changed")


def verify_loaded_origins(manifest: dict, getter=None) -> list[dict]:
    expected = {str((HERE / item["relativePath"]).resolve()): item["sha256"]
                for item in manifest["upstreamFiles"]}
    origins = []
    for name, module in sorted(sys.modules.copy().items()):
        if name != "jevkit" and not name.startswith("jevkit."):
            continue
        filename = getattr(module, "__file__", None)
        require(isinstance(filename, str), "upstream_module_origin_missing")
        path = str(Path(filename).resolve())
        require(path in expected, "upstream_module_origin_mismatch")
        observed = digest(Path(path).read_bytes())
        require(observed == expected[path], "loaded_upstream_source_changed")
        origins.append({"module": name, "path": path, "sha256": observed})
    require({"jevkit", "jevkit.client", "jevkit.rerank"} <= {item["module"] for item in origins}, "upstream_modules_missing")
    if getter is None:
        return origins
    require(getattr(getter, "__module__", None) == "hermes_cli.config", "native_getter_origin_mismatch")
    native = sys.modules.get("hermes_cli.config")
    filename = getattr(native, "__file__", None)
    require(isinstance(filename, str) and str(Path(filename).resolve()) == "/opt/hermes/hermes_cli/config.py", "native_module_origin_mismatch")
    require(str(Path(getter.__code__.co_filename).resolve()) == "/opt/hermes/hermes_cli/config.py", "native_getter_code_origin_mismatch")
    native_sha = next(item["sha256"] for item in manifest["installedNativeSourcePins"]
                      if item["path"] == "/opt/hermes/hermes_cli/config.py")
    require(digest(Path(filename).read_bytes()) == native_sha, "loaded_native_source_changed")
    origins.append({"module": "hermes_cli.config", "path": "/opt/hermes/hermes_cli/config.py", "sha256": native_sha})
    return origins


def require_native_smoke_score(raw_answers: object, parsed: dict) -> None:
    require(isinstance(raw_answers, dict), "raw_smoke_answers_missing")
    raw = raw_answers.get("specificity")
    require(isinstance(raw, dict) and raw.get("type") == "score", "raw_smoke_score_missing")
    require(number(raw.get("confidence")) and 0 <= raw["confidence"] <= 1, "native_score_confidence_missing")
    require(number(raw.get("score")) and 0 <= raw["score"] <= 2, "native_score_outside_levels")
    probabilities = raw.get("probabilities")
    require(isinstance(probabilities, dict) and set(probabilities) == {"0", "1", "2"}, "native_score_distribution_incomplete")
    require(all(number(value) and 0 <= value <= 1 for value in probabilities.values()), "native_score_probability_invalid")
    # Upstream already checks probability mass and score/mean agreement. These
    # extra native checks forbid absent metadata/default confidence and omissions.
    require(parsed.get("spread_reported") is True, "native_score_spread_missing")


def read_request() -> tuple[dict, str]:
    data = sys.stdin.buffer.read(16_385)
    require(0 < len(data) <= 16_384, "request_size")
    request = json.loads(data)
    require(isinstance(request, dict), "request_shape")
    require(set(request) <= {"schema", "operation", "observationId", "budget", "query", "candidates", "topK", "workerObservation", "previousObservationDigest"}, "request_fields")
    require(request.get("schema") == "host-orchestrator.jev-call.v1", "request_schema")
    require(request.get("operation") in {"smoke", "rank", "assess_worker"}, "operation")
    require(isinstance(request.get("observationId"), str) and LABEL.fullmatch(request["observationId"]), "observation_id")
    budget = request.get("budget")
    require(isinstance(budget, dict), "budget_missing")
    require(set(budget) == {"maxHttpRequests", "timeoutSeconds", "reserveUsd", "priorAccountedUsd", "cumulativeCapUsd", "rootReceiptSha256"}, "budget_fields")
    require(type(budget["maxHttpRequests"]) is int and budget["maxHttpRequests"] == 1, "http_budget")
    require(number(budget["timeoutSeconds"]) and 1 <= budget["timeoutSeconds"] <= 30, "timeout_budget")
    require(number(budget["reserveUsd"]) and 0 < budget["reserveUsd"] <= 0.10, "reserve_budget")
    require(number(budget["priorAccountedUsd"]) and budget["priorAccountedUsd"] >= 0, "accounting_missing")
    require(budget["cumulativeCapUsd"] == 14 and not isinstance(budget["cumulativeCapUsd"], bool), "cap_changed")
    require(budget["priorAccountedUsd"] + budget["reserveUsd"] <= 14, "cap_exceeded")
    require(isinstance(budget["rootReceiptSha256"], str) and HEX.fullmatch(budget["rootReceiptSha256"]), "root_receipt_missing")
    if request["operation"] == "assess_worker":
        require(not ({"query", "candidates", "topK"} & set(request)), "worker_fields")
        require("previousObservationDigest" in request, "previous_observation_missing")
        previous = request["previousObservationDigest"]
        require(previous is None or (isinstance(previous, str) and HEX.fullmatch(previous)), "previous_observation_digest")
        validate_worker_observation(request.get("workerObservation"))
    elif request["operation"] == "smoke":
        require(not ({"workerObservation", "previousObservationDigest"} & set(request)), "smoke_fields")
        require(not ({"query", "candidates", "topK"} & set(request)), "smoke_is_fixed")
    else:
        require(not ({"workerObservation", "previousObservationDigest"} & set(request)), "rank_fields")
        query = request.get("query")
        candidates = request.get("candidates")
        require(isinstance(query, str) and 1 <= len(query) <= 500, "query_bound")
        require(isinstance(candidates, list) and 2 <= len(candidates) <= 8, "candidate_bound")
        require(type(request.get("topK")) is int and 1 <= request["topK"] <= len(candidates), "top_k")
        ids = []
        for candidate in candidates:
            require(isinstance(candidate, dict) and set(candidate) == {"id", "text", "textSha256"}, "candidate_shape")
            require(isinstance(candidate["id"], str) and LABEL.fullmatch(candidate["id"]), "candidate_id")
            require(isinstance(candidate["text"], str) and 1 <= len(candidate["text"]) <= 900, "candidate_text_bound")
            require(digest(candidate["text"].encode()) == candidate["textSha256"], "observation_hash_mismatch")
            ids.append(candidate["id"])
        require(len(set(ids)) == len(ids), "duplicate_candidate")
    return request, digest(data)


def validate_worker_observation(observation: object) -> None:
    require(isinstance(observation, dict) and set(observation) == {
        "native", "logical", "phase", "liveness", "eventKind", "eventEvidenceSha256",
        "goal", "output", "elapsedSeconds", "exitCode",
    }, "worker_observation_shape")
    native = observation["native"]
    require(isinstance(native, dict) and set(native) == {
        "runtime", "profileId", "sessionId", "agentId", "nativeProjectId", "nativeTaskId", "nativeRunId",
    }, "worker_native_shape")
    require(native["runtime"] in {"native-hermes", "native-codex"}, "worker_runtime")
    for key in ("profileId", "sessionId", "agentId", "nativeProjectId", "nativeTaskId", "nativeRunId"):
        value = native[key]
        require(value is None or (isinstance(value, str) and LABEL.fullmatch(value)), "worker_native_id")
    require(native["sessionId"] is not None, "worker_session_missing")
    require(native["runtime"] != "native-hermes" or native["profileId"] is not None, "worker_profile_missing")
    logical = observation["logical"]
    require(isinstance(logical, dict) and set(logical) == {"domainId", "projectId", "workId", "taskId"}, "worker_logical_shape")
    require(all(isinstance(value, str) and LABEL.fullmatch(value) for value in logical.values()), "worker_logical_id")
    require(isinstance(observation["phase"], str) and LABEL.fullmatch(observation["phase"]), "worker_phase")
    require(observation["liveness"] in WORKER_LIVENESS, "worker_liveness")
    require(observation["eventKind"] in WORKER_EVENTS, "worker_event_kind")
    require(isinstance(observation["eventEvidenceSha256"], str) and HEX.fullmatch(observation["eventEvidenceSha256"]), "worker_event_evidence")
    require(isinstance(observation["goal"], str) and 1 <= len(observation["goal"]) <= 600, "worker_goal_bound")
    require(isinstance(observation["output"], str) and len(observation["output"]) <= 2400, "worker_output_bound")
    require(number(observation["elapsedSeconds"]) and 0 <= observation["elapsedSeconds"] <= 2678400, "worker_elapsed_bound")
    exited = observation["exitCode"]
    if observation["eventKind"] == "native_exit":
        require(type(exited) is int and -255 <= exited <= 255 and observation["liveness"] == "exited", "worker_exit_evidence")
    else:
        require(exited is None and observation["liveness"] != "exited", "worker_exit_without_event")


def worker_observation_digest(observation: dict) -> str:
    # Time passing and a different carrier-event hash are not material progress.
    # The Root supplies its previous task-local receipt digest; no registry/cache is created.
    material = {key: observation[key] for key in (
        "native", "logical", "phase", "liveness", "eventKind", "goal", "output", "exitCode",
    )}
    return digest(json.dumps(material, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode())


def worker_result(supervise, observation: dict, transport, timeout: float) -> dict:
    tail = ("Observed native event: phase=" + observation["phase"] +
            " liveness=" + observation["liveness"] + " event=" + observation["eventKind"] +
            "\nSanitized owned worker output:\n" + observation["output"])
    require(len(tail) <= supervise.TAIL_CHARS, "worker_rendered_output_bound")
    snapshot = supervise.assess(
        observation["goal"], tail, elapsed_s=observation["elapsedSeconds"],
        quiet_s=0.0, new_output=observation["eventKind"] == "output_changed",
        looping=False, exited=observation["exitCode"], transport=transport, timeout=timeout,
    )
    if snapshot.jev_error is not None:
        code = snapshot.jev_error
        raise Refusal(code if isinstance(code, str) and re.fullmatch(r"[a-z0-9_]{1,64}", code) else "upstream_error")
    require(snapshot.action in WORKER_ACTIONS, "worker_action_membership")
    require(number(snapshot.confidence) and 0 <= snapshot.confidence <= 1, "worker_confidence")
    probabilities = {key: getattr(snapshot, key) for key in ("progressing", "needs_input", "blocked", "done")}
    require(all(value is None or (number(value) and 0 <= value <= 1) for value in probabilities.values()), "worker_probability")
    if observation["exitCode"] is None:
        require(all(value is not None for value in probabilities.values()), "worker_probability_missing")
    return {
        "decisionSource": "native_exit" if observation["exitCode"] is not None else "jev",
        "actionAdvice": snapshot.action, "confidence": snapshot.confidence,
        "probabilities": probabilities, "injectionSeen": bool(snapshot.injection_seen),
        "executionAuthorityGranted": False, "workerCompletionVerified": False, "automaticActionTaken": False,
        "interpretation": "Advice only. collect requests fresh native result and task-postcondition inspection; silence creates no stuck/completion evidence.",
    }


def usage_only(value: object) -> dict:
    if not isinstance(value, dict):
        return {}
    return {key: int(value[key]) for key in ("input_tokens", "output_tokens", "total_tokens")
            if key in value and number(value[key]) and value[key] >= 0 and float(value[key]).is_integer()}


class SingleRequestPool:
    """Budget-only facade over the exact upstream pool; never opens a connection."""
    def __init__(self, original, error):
        self.original, self.error, self.leases = original, error, 0

    def borrow(self, key, timeout):
        # Upstream transport has an additional two-attempt stale-socket path even
        # with ask(retries=0). Refuse its second borrow BEFORE network activity.
        if self.leases:
            raise self.error("http_budget_exhausted")
        self.leases += 1
        return self.original.borrow(key, timeout)

    def release(self, key, connection):
        return self.original.release(key, connection)

    def drop(self, connection):
        return self.original.drop(connection)


def run() -> dict:
    manifest_bytes = (HERE / "application-manifest.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    require(os.getuid() == 10000, "wrong_runtime_uid")
    require(os.environ.get("HOME") == "/opt/data" and os.environ.get("HERMES_HOME") == "/opt/data", "wrong_native_home")
    require(str(HERE) == manifest["deploymentRoot"], "wrong_deployment_root")
    verify_sources(manifest)
    request, request_sha = read_request()
    sys.path.insert(0, str(HERE / "upstream"))
    from jevkit import client, rerank, supervise, privacy
    require(sys.modules.get("jevkit.client") is client and sys.modules.get("jevkit.rerank") is rerank, "upstream_module_object_mismatch")
    require(sys.modules.get("jevkit.supervise") is supervise, "worker_module_object_mismatch")
    loaded_origins = verify_loaded_origins(manifest)
    worker = request.get("workerObservation")
    observation_digest = worker_observation_digest(worker) if worker is not None else None
    duplicate = worker is not None and request["previousObservationDigest"] == observation_digest
    native_exit = worker is not None and worker["exitCode"] is not None
    if worker is not None and not duplicate and not native_exit:
        require(not privacy.is_sensitive(worker["goal"]) and not privacy.is_sensitive(worker["output"]), "worker_input_sensitive")
    needs_network = not duplicate and not native_exit
    get_env_value_prefer_dotenv = None

    # No Hermes main/session/configuration activation and no secret-source hydration.
    if needs_network:
        sys.path.insert(0, "/opt/hermes")
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            from hermes_cli.config import get_env_value_prefer_dotenv
            loaded_origins = verify_loaded_origins(manifest, get_env_value_prefer_dotenv)
            key = get_env_value_prefer_dotenv("AI_GATEWAY_API_KEY")
        require(bool(key) and not str(key).lstrip().startswith("op://"), "native_gateway_credential_unavailable")
        os.environ["JEV_PROXY_API_KEY"] = key
        os.environ["TYPESAFE_BASE_URL"] = BASE
        os.environ["TYPESAFE_MODEL"] = MODEL
        os.environ["JEV_PROVIDER"] = "typesafe"
        del key

    pool = SingleRequestPool(client._POOL, client.JevError)
    client._POOL = pool
    transport_calls, reply_meta = 0, {}

    def transport(body, headers, timeout):
        nonlocal transport_calls
        if transport_calls:
            raise client.JevError("http_budget_exhausted")
        transport_calls += 1
        raw = client._http_transport(body, headers, timeout, ENDPOINT, 65_536)
        payload = json.loads(raw)
        if isinstance(payload, dict):
            reply_meta["model"] = payload.get("model")
            reply_meta["usage"] = usage_only(payload.get("usage"))
            reply_meta["rawAnswers"] = payload.get("answers")
        return raw

    started = time.monotonic()
    failure = None
    operation_result = {}
    try:
        if duplicate:
            operation_result = {"decisionSource": "not_requested", "reason": "duplicate_observation",
                                "executionAuthorityGranted": False, "workerCompletionVerified": False, "automaticActionTaken": False}
        elif request["operation"] == "assess_worker":
            operation_result = worker_result(supervise, worker, transport, request["budget"]["timeoutSeconds"])
        elif request["operation"] == "smoke":
            state = {"component": "evidence-helper", "observed_status": "ready", "candidate": "read-source", "changes_permitted": False}
            questions = {
                "ready": client.noul("Is `observed_status` ready?"),
                "candidate": client.choice("Which candidate is explicitly present in state?", {"read-source": "Read source evidence", "change-service": "Change a running service", "none": "Neither candidate is present"}),
                "specificity": client.score("How specific is the named candidate in state?", ["No candidate", "A generic candidate", "A named source-reading candidate"]),
            }
            result = client.ask(state, questions, timeout=request["budget"]["timeoutSeconds"], retries=0,
                                model=MODEL, provider="typesafe", transport=transport)
            require_native_smoke_score(reply_meta.get("rawAnswers"), result["answers"]["specificity"])
            answers = {name: {k: v for k, v in answer.items() if k in {"type", "noul", "choice", "score", "probabilities", "confidence", "spread_reported"}}
                       for name, answer in result["answers"].items()}
            operation_result = {"submittedSyntheticState": state, "answers": answers}
        else:
            result = rerank.rerank(request["query"], request["candidates"], top_k=request["topK"],
                                   timeout=request["budget"]["timeoutSeconds"], transport=transport)
            allowed_ids = {candidate["id"] for candidate in request["candidates"]}
            for field in ("selected_ids", "dropped_injection_ids", "local_screen_ids", "unjudged_ids", "clipped_ids"):
                require(isinstance(result.get(field), list) and set(result[field]) <= allowed_ids, "rank_membership")
            operation_result = {k: result[k] for k in ("status", "screening", "selected_ids", "dropped_injection_ids", "local_screen_ids", "unjudged_ids", "clipped_ids", "truncated", "top_k", "scores")}
            require(result["status"] == "ok" and not result["unjudged_ids"], "rank_incomplete")
        if needs_network:
            require(reply_meta.get("model") in MODELS_SEEN, "returned_model_mismatch")
    except client.JevError as error:
        failure = error.code if re.fullmatch(r"[a-z0-9_]{1,64}", error.code) else "upstream_error"
    except Refusal as error:
        failure = error.args[0]
    except Exception:
        failure = "caller_or_schema_error"
    finally:
        os.environ.pop("JEV_PROXY_API_KEY", None)
    # Fresh post-call source observation; no interpretation grants execution authority.
    source_postcondition = False
    try:
        verify_sources(manifest)
        require(verify_loaded_origins(manifest, get_env_value_prefer_dotenv) == loaded_origins, "loaded_origins_changed")
        require(digest((HERE / "application-manifest.json").read_bytes()) == digest(manifest_bytes), "manifest_changed")
        source_postcondition = True
    except Refusal as error:
        failure = error.args[0]
    except Exception:
        failure = "source_postcondition_unavailable"
    if worker is not None and not failure:
        operation_result.update({"observationDigest": observation_digest,
                                 "workerBinding": {key: worker[key] for key in ("native", "logical", "phase", "liveness", "eventKind", "eventEvidenceSha256")}})
    return {
        "schema": "host-orchestrator.jev-call-result.v1", "operation": request["operation"],
        "observationId": request["observationId"], "submittedRequestSha256": request_sha,
        "status": "error" if failure else ("skipped" if duplicate else "ok"), "errorCode": failure,
        "requestedModel": MODEL, "returnedModel": reply_meta.get("model") if reply_meta.get("model") in MODELS_SEEN else None,
        "usage": reply_meta.get("usage", {}), "callerLatencyMs": round((time.monotonic() - started) * 1000),
        "transportInvocations": transport_calls, "connectionLeases": pool.leases, "maxHttpRequests": 1,
        "budget": request["budget"], "billingCostUsd": None,
        "outcomeRule": "Reconcile this exact receipt before any retry; a transport failure may have consumed a paid request.",
        "nativeChecks": {"uid": 10000, "home": "/opt/data", "sourcePinsBeforeAndAfter": source_postcondition,
                         "loadedModuleOrigins": loaded_origins,
                         "manifestSha256": digest(manifest_bytes), "nativeTaskOrRunIds": None,
                         "mainModelOrEffortChanged": False, "executionEffects": False},
        "result": operation_result if not failure else {},
    }


if __name__ == "__main__":
    try:
        output = run()
    except Refusal as error:
        output = {"schema": "host-orchestrator.jev-call-result.v1", "status": "error", "errorCode": error.args[0], "retryAllowed": False}
    except Exception:
        output = {"schema": "host-orchestrator.jev-call-result.v1", "status": "error", "errorCode": "native_preflight_or_postcondition_error", "retryAllowed": False}
    print(json.dumps(output, separators=(",", ":"), allow_nan=False), flush=True)
    raise SystemExit(0 if output["status"] in {"ok", "skipped"} else 1)
