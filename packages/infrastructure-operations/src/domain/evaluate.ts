import { assertObservation } from "./validation";
import type { InfrastructureObservation, GatewayProfileServing, RuntimeObservation, VerificationResultProperties } from "./types";

export interface EvaluationOptions {
  nowUtc?: string;
  maxObservationAgeSeconds?: number;
  maxStatusAgeSeconds?: number;
}
export interface InfrastructureEvaluation {
  controlPlanePassed: boolean;
  allProfilesOn: boolean;
  checks: {
    currentObservation: boolean;
    sharedVolumeConsistent: boolean;
    exactSingletonLockHeld: boolean;
    intendedRuntimeOwnsLock: boolean;
    freshIdentityMatchesHolder: boolean;
    freshIdentityServesProfile: boolean;
    nativeServingConfirmed: boolean;
    nativeProfileOn: boolean;
    allExpectedProfilesNativeOn: boolean;
    settledRuntimeState: boolean;
    liveStatusMatchesHolder: boolean;
    oneRootGatewaySlotUp: boolean;
    preservedSessionAlive: boolean;
  };
  profileResults: GatewayProfileServing[];
  adapterFailures: RuntimeObservation["native"]["adapters"];
  allAdaptersHealthy: boolean;
}

/** Pure deterministic verification. Never grants permission or produces acceptance. */
export function evaluateInfrastructure(observation: InfrastructureObservation, options: EvaluationOptions = {}): InfrastructureEvaluation {
  assertObservation(observation);
  const now = Date.parse(options.nowUtc ?? new Date().toISOString());
  const maxObservationAge = options.maxObservationAgeSeconds ?? 60;
  const maxStatusAge = options.maxStatusAgeSeconds ?? 60;
  if (!Number.isFinite(now) || !Number.isFinite(maxObservationAge) || maxObservationAge < 0 || !Number.isFinite(maxStatusAge) || maxStatusAge < 0) throw new Error("Invalid deterministic verification time window.");
  const observedSeconds = Date.parse(observation.observedAtUtc) / 1000;
  const ageSeconds = now / 1000 - observedSeconds;
  const candidates = observation.runtimes.filter(runtime => runtime.service === observation.targetService);
  const target = candidates.length === 1 ? candidates[0] : undefined;
  const holder = observation.lock.holders.length === 1 ? observation.lock.holders[0] : undefined;
  const identity = target?.native.identify;
  const expectedProfiles = observation.expectedProfiles ?? [observation.profile];
  const profileResults = target?.native.profiles ?? (target ? [{
    profile: observation.profile,
    nativeOn: target.native.profileOn,
    liveServed: identity?.served_profiles?.includes(observation.profile) === true,
    nativeOwnerPid: target.native.nativeOwnerPid ?? null,
  }] : []);
  const allProfilesOn = expectedProfiles.every(name => {
    const result = profileResults.find(profile => profile.profile === name);
    return result?.nativeOn === true && result.liveServed === true && !!holder
      && result.nativeOwnerPid === holder.localPid && identity?.served_profiles?.includes(name) === true;
  });
  const checks = {
    currentObservation: ageSeconds >= -5 && ageSeconds <= maxObservationAge,
    sharedVolumeConsistent: !!target && observation.runtimes.every(runtime => runtime.dataSource === target.dataSource),
    exactSingletonLockHeld: observation.lock.held && observation.lock.holders.length === 1 && (holder?.matchingFdCount ?? 0) > 0,
    intendedRuntimeOwnsLock: !!target && holder?.containerId === target.containerId && holder.namespace === target.namespace,
    freshIdentityMatchesHolder: identity?.kind === "hermes-gateway" && !!holder && identity.pid === holder.localPid && identity.start_time === holder.startTicks,
    freshIdentityServesProfile: identity?.served_profiles?.includes(observation.profile) === true,
    nativeServingConfirmed: target?.native.nativeServing === true && !!holder && target.native.nativeOwnerPid === holder.localPid,
    nativeProfileOn: target?.native.profileOn === true,
    allExpectedProfilesNativeOn: allProfilesOn,
    settledRuntimeState: ["running", "degraded"].includes(target?.native.status.gateway_state ?? "") && target?.native.status.desired_state === "running",
    liveStatusMatchesHolder: !!holder && target?.native.status.answering_pid === holder.localPid && Math.abs(observedSeconds - (target?.native.status.answered_at ?? 0)) < maxStatusAge,
    oneRootGatewaySlotUp: observation.runtimes.filter(runtime => runtime.rootSlot.up).length === 1 && !!holder && target?.rootSlot.localPid === holder.localPid,
    preservedSessionAlive: observation.preservedSession?.alive ?? true,
  };
  const adapterFailures = target?.native.adapters.filter(adapter => adapter.configured !== false && (adapter.credentialRejected || ["fatal", "failed", "retrying", "disconnected"].includes(adapter.state ?? ""))) ?? [];
  return {
    controlPlanePassed: Object.values(checks).every(Boolean), allProfilesOn, checks, profileResults,
    adapterFailures,
    allAdaptersHealthy: !!target && adapterFailures.length === 0 && (target.native.adapterErrors?.length ?? 0) === 0 && target.native.status.gateway_state === "running",
  };
}

/** Construct a verification payload; acceptance must be recorded independently. */
export function verificationFromEvaluation(
  evaluation: InfrastructureEvaluation,
  context: { hostId: string; observationRef: string; verifiedAtUtc: string; limitations?: string[] },
): VerificationResultProperties {
  return {
    hostId: context.hostId, observationRefs: [context.observationRef],
    verificationClass: "current-runtime", verifiedAtUtc: context.verifiedAtUtc,
    status: evaluation.controlPlanePassed ? "passed" : "failed",
    criteria: Object.entries(evaluation.checks).map(([id, passed]) => ({
      id, status: passed ? "pass" : "fail", description: id, observationRefs: [context.observationRef],
    })),
    adapterHealth: evaluation.allAdaptersHealthy ? "healthy" : "degraded",
    limitations: context.limitations ?? [],
  };
}
