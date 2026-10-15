import { describe, expect, test } from "bun:test";
import { evaluateInfrastructure, verificationFromEvaluation, validateDomainProperties, validateObservation } from "../src/domain";
import { NOW, HOST, SIBLING, snapshot } from "./domain.fixtures";

describe("deterministic gateway verification", () => {
  const verify = (data: ReturnType<typeof snapshot>, nowUtc = NOW) => evaluateInfrastructure(data, { nowUtc });

  test("exact owner and all six native On profiles pass despite the sibling's Off view", () => {
    const result = verify(snapshot());
    expect(result.controlPlanePassed).toBe(true);
    expect(result.allProfilesOn).toBe(true);
    expect(result.allAdaptersHealthy).toBe(true);
    expect(result.profileResults).toHaveLength(6);
    const payload = verificationFromEvaluation(result, { hostId: HOST, observationRef: "observed-snapshot", verifiedAtUtc: NOW });
    expect(payload.status).toBe("passed");
    expect(validateDomainProperties("VerificationResult", payload).valid).toBe(true);
    expect("authority" in payload).toBe(false);
  });

  test("one intended runtime retains singleton and all-six native profile verification", () => {
    const data = snapshot(); data.runtimes = data.runtimes.filter(runtime => runtime.service === 'hermes-serve');
    expect(validateObservation(data).valid).toBe(true);
    expect(verify(data).controlPlanePassed).toBe(true);
    expect(verify(data).allProfilesOn).toBe(true);
    data.lock.holders[0]!.containerId = SIBLING;
    expect(verify(data).controlPlanePassed).toBe(false);
  });

  test("a foreign namespace can see the same fresh socket but cannot pass intended backend acceptance", () => {
    const data = snapshot();
    data.lock.holders[0]!.containerId = SIBLING;
    data.lock.holders[0]!.namespace = data.runtimes[0]!.namespace;
    data.runtimes[1]!.native.nativeServing = false;
    data.runtimes[1]!.native.nativeOwnerPid = null;
    data.runtimes[1]!.native.profileOn = false;
    const result = verify(data);
    expect(result.controlPlanePassed).toBe(false);
    expect(result.checks.freshIdentityMatchesHolder).toBe(true);
    expect(result.checks.intendedRuntimeOwnsLock).toBe(false);
    expect(result.checks.nativeServingConfirmed).toBe(false);
  });

  test("Docker healthy, s6 up, HTTP 200 and a served list cannot replace native serving proof", () => {
    const data = snapshot();
    data.runtimes[1]!.native.nativeServing = false;
    data.runtimes[1]!.native.profileOn = false;
    expect(verify(data).controlPlanePassed).toBe(false);
    data.runtimes[1]!.native.nativeServing = true;
    data.runtimes[1]!.native.profileOn = true;
    data.runtimes[1]!.native.identify = null;
    expect(verify(data).controlPlanePassed).toBe(false);
  });

  test("PID reuse and missing exact file ownership fail independently of a connectable socket", () => {
    const reused = snapshot();
    reused.runtimes[1]!.native.identify!.start_time = 9999;
    expect(verify(reused).checks.freshIdentityMatchesHolder).toBe(false);
    expect(verify(reused).controlPlanePassed).toBe(false);
    const noExactFd = snapshot();
    noExactFd.lock.holders[0]!.matchingFdCount = 0;
    expect(verify(noExactFd).controlPlanePassed).toBe(false);
  });

  test("an unserved or native-Off profile prevents all-profile acceptance", () => {
    const data = snapshot();
    data.runtimes[1]!.native.profiles![5]!.nativeOn = false;
    expect(verify(data).allProfilesOn).toBe(false);
    expect(verify(data).controlPlanePassed).toBe(false);
    data.runtimes[1]!.native.profiles![5]!.nativeOn = true;
    data.runtimes[1]!.native.identify!.served_profiles!.pop();
    expect(verify(data).allProfilesOn).toBe(false);
    const wrongOwner = snapshot();
    wrongOwner.runtimes[1]!.native.profiles![5]!.nativeOwnerPid = 999;
    expect(verify(wrongOwner).allProfilesOn).toBe(false);
  });

  test("all-profile verification requires explicit per-profile native On measurements", () => {
    const data = snapshot();
    delete data.runtimes[1]!.native.profiles;
    expect(verify(data).allProfilesOn).toBe(false);
    delete data.expectedProfiles;
    expect(verify(data).allProfilesOn).toBe(true);
  });

  test("credential adapter failures remain separate from successful gateway serving", () => {
    const data = snapshot();
    data.runtimes[1]!.native.status.gateway_state = "degraded";
    data.runtimes[1]!.native.adapters.push({ name: "telegram", state: "fatal", errorCode: "telegram_auth_error", credentialRejected: true });
    const result = verify(data);
    expect(result.controlPlanePassed).toBe(true);
    expect(result.allAdaptersHealthy).toBe(false);
    expect(result.adapterFailures).toHaveLength(1);
  });

  test("an explicitly disabled obsolete fatal record is preserved without degrading active adapters", () => {
    const data = snapshot();
    data.runtimes[1]!.native.adapters.push({ name: "telegram", state: "fatal", errorCode: "telegram_auth_error", credentialRejected: true, configured: false, writerPid: 14661 });
    data.runtimes[1]!.native.adapters.push({ name: "api_server", state: "connected", errorCode: null, credentialRejected: false, configured: true, writerPid: 206 });
    const result = verify(data);
    expect(result.controlPlanePassed).toBe(true);
    expect(result.allAdaptersHealthy).toBe(true);
    expect(result.adapterFailures).toHaveLength(0);
    expect(data.runtimes[1]!.native.adapters.find(adapter => adapter.name === "telegram")!.state).toBe("fatal");
    expect(validateObservation(data).valid).toBe(true);
  });

  test("fatal adapters with active or unknown configuration continue to fail health", () => {
    for (const configured of [undefined, true] as const) {
      const data = snapshot();
      data.runtimes[1]!.native.adapters.push({ name: "telegram", state: "fatal", errorCode: "telegram_auth_error", credentialRejected: true, writerPid: null, ...(configured === undefined ? {} : { configured }) });
      const result = verify(data);
      expect(result.controlPlanePassed).toBe(true);
      expect(result.allAdaptersHealthy).toBe(false);
      expect(result.adapterFailures).toHaveLength(1);
    }
  });

  test("an old snapshot or stale live status cannot be presented as current verification", () => {
    expect(verify(snapshot(), "2026-10-06T12:02:00Z").checks.currentObservation).toBe(false);
    expect(verify(snapshot(), "2026-10-06T11:58:00Z").checks.currentObservation).toBe(false);
    const stale = snapshot();
    stale.runtimes[1]!.native.status.answered_at! -= 120;
    expect(verify(stale).controlPlanePassed).toBe(false);
  });

  test("duplicate root gateway slots and lost preserved process fail control-plane acceptance", () => {
    const duplicate = snapshot();
    duplicate.runtimes[0]!.rootSlot.up = true;
    duplicate.runtimes[0]!.rootSlot.localPid = 206;
    expect(verify(duplicate).controlPlanePassed).toBe(false);
    const lostSession = snapshot();
    lostSession.preservedSession!.alive = false;
    expect(verify(lostSession).controlPlanePassed).toBe(false);
  });

  test("ambiguous target replicas or differing shared-volume sources require resolution", () => {
    const replicas = snapshot();
    const duplicate = structuredClone(replicas.runtimes[1]!);
    duplicate.containerId = "f".repeat(64);
    duplicate.rootSlot.up = false;
    replicas.runtimes.push(duplicate);
    expect(verify(replicas).controlPlanePassed).toBe(false);
    const differentVolume = snapshot();
    differentVolume.runtimes[0]!.dataSource = "/docker/other/data";
    expect(verify(differentVolume).checks.sharedVolumeConsistent).toBe(false);
  });

  test("HTTP 200 with nullable gateway state preserves a stopped transition without proving serving", () => {
    const stopped = snapshot();
    stopped.lock.held = false;
    stopped.lock.holders = [];
    const target = stopped.runtimes[1]!;
    target.rootSlot = { up: false, localPid: null, raw: "down (exitcode 0) 1 second, normally up", downFileExists: false };
    target.native.identify = null;
    target.native.nativeServing = false;
    target.native.nativeOwnerPid = null;
    target.native.profileOn = false;
    target.native.profiles = target.native.profiles!.map(profile => ({ ...profile, nativeOn: false, liveServed: false, nativeOwnerPid: null }));
    target.native.status = {};
    target.native.adapters = [];
    target.native.http = { code: 200, gateway_running: false, gateway_state: null, gateway_mode: "none" };
    const validated = validateObservation(stopped);
    expect(validated.valid).toBe(true);
    expect(validated.valid && validated.value.runtimes[1]!.native.http!.gateway_state === null).toBe(true);
    const result = verify(stopped);
    expect(result.controlPlanePassed).toBe(false);
    expect(result.allProfilesOn).toBe(false);
    expect(result.allAdaptersHealthy).toBe(false);
    expect(result.checks.exactSingletonLockHeld).toBe(false);
    expect(result.checks.nativeServingConfirmed).toBe(false);
    target.native.http.gateway_state = 0 as unknown as string;
    expect(validateObservation(stopped).valid).toBe(false);
  });
});
