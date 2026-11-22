//! The detector `credit` of `ap watch` (`specs/ap/watch.md` R31): the API credit of `specs/ap/api.md`. The
//! month's counted spend against each stop of the policy's member `api`, the batch runs still running 26 hours
//! after their start, and the mode of the key file, read by `stat` alone. The amounts are those of
//! `crates/ap/src/api.rs` (`batch_runs`, `tally`, `month_of`, `sum`): no second rule of the spend is written here.
use super::detect::regular;
use super::{Read, Reading, Signal, epoch, utc};
use crate::api::{batch_runs, config, month_of, sum, tally};
use crate::host::{Answer, Network, System, WayOut};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

/// The families of `credit` (R7).
pub const FAMILIES: [&str; 3] = ["stop", "unsettled", "key mode"];
/// A running batch run is unsettled when it started more than this before the pass.
const UNSETTLED_AFTER: u64 = 26 * 3_600;
/// A stop is signalled at 80 % of it: micro-USD per whole USD of the stop.
const STOP_SHARE: i64 = 800_000;
const MICROS: i64 = 1_000_000;
const MS: u64 = 10_000;

/// The port of the pass, which also keeps the native keys of the objects each answer of `ont search` holds, in
/// their order: `batch_runs` gives the fields of the runs in the order of the search's answers but not their keys,
/// and the signature of `unsettled` names the key. Every other method is the pass's port as it is.
struct Keys<'a, S: System> {
    port: &'a mut S,
    keys: Vec<String>,
}
impl<S: System> System for Keys<'_, S> {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        let answer = self.port.call(program, args, input, ms);
        let search = program == "ont" && args.first().is_some_and(|a| a == "search");
        if let (true, Ok(reply)) = (search, &answer) {
            let v: Value = serde_json::from_slice(&reply.bytes).unwrap_or_default();
            if reply.success && v["status"] == "ok" {
                let objects = v["result"]["objects"].as_array().into_iter().flatten();
                let keys = objects.map(|o| o["id"]["native_key"].as_str().unwrap_or_default());
                self.keys.extend(keys.map(str::to_owned));
            }
        }
        answer
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.port.read(path)
    }
    fn work_root(&self) -> PathBuf {
        self.port.work_root()
    }
    fn home(&self) -> Option<PathBuf> {
        self.port.home()
    }
    fn cwd(&self) -> std::io::Result<PathBuf> {
        self.port.cwd()
    }
    fn directory(&self, path: &str) -> std::io::Result<PathBuf> {
        self.port.directory(path)
    }
    fn now(&self) -> Instant {
        self.port.now()
    }
    fn wall(&self) -> SystemTime {
        self.port.wall()
    }
    fn pause(&mut self, duration: Duration) {
        self.port.pause(duration);
    }
    fn launch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cwd: &str,
        files: [std::fs::File; 3],
    ) -> std::io::Result<std::process::Child> {
        self.port.launch(argv, env, cwd, files)
    }
    fn units(&self) -> bool {
        self.port.units()
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.port.policy_file()
    }
    fn git_hooks(&self) -> Option<PathBuf> {
        self.port.git_hooks()
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.port.environment()
    }
    fn network(&mut self) -> std::io::Result<Network> {
        self.port.network()
    }
    fn way_out(&self) -> Arc<dyn WayOut> {
        self.port.way_out()
    }
}

/// Every run of role `batch` with its native key: the one search of `batch_runs` from 0 (R31).
fn runs(s: &mut impl System) -> Result<Vec<(String, Value)>, String> {
    let mut port = Keys {
        port: s,
        keys: vec![],
    };
    let runs = batch_runs(&mut port, None, 0)
        .map_err(|f| format!("ont search run failed: {}", crate::store::code(&f)))?;
    if port.keys.len() != runs.len() || port.keys.iter().any(String::is_empty) {
        return Err("ont search run: the keys of the runs could not be read".to_owned());
    }
    Ok(port.keys.into_iter().zip(runs).collect())
}

/// `stop <queue>`: the month's counted spend at 80 % of a stop or more.
fn stops(now: u64, runs: &[(String, Value)], stops: [(&str, u64); 2], out: &mut Vec<Signal>) {
    let Some(month) = month_of(now) else {
        return;
    };
    let fields: Vec<Value> = runs.iter().map(|(_, f)| f.clone()).collect();
    let counted = sum(&tally(&fields, month));
    let name = &utc(month.0)[..7];
    for (queue, stop) in stops {
        let stop = i64::try_from(stop).unwrap_or(i64::MAX);
        if counted >= stop.saturating_mul(STOP_SHARE) {
            let micros = stop.saturating_mul(MICROS);
            out.push(Signal::new(
                format!("stop {queue}"),
                "act",
                &format!(
                    "the API credit counted in {name} has reached 80 % of the stop of the queue {queue}"
                ),
                format!("month {name}, counted_usd_micros {counted}, stop_usd_micros {micros}"),
            ));
        }
    }
}

/// `unsettled <run key>`: a batch run not closed more than 26 hours after its start.
fn unsettled(now: u64, runs: &[(String, Value)], out: &mut Vec<Signal>) {
    for (key, f) in runs {
        if f["status"] == "closed" {
            continue;
        }
        let Some(started) = f["started_at"].as_str() else {
            continue;
        };
        if !epoch(started).is_some_and(|t| now.saturating_sub(t) > UNSETTLED_AFTER) {
            continue;
        }
        let job = f["job"].as_str().unwrap_or_default();
        let reserved = f["reserved_usd_micros"].as_i64().unwrap_or(0);
        out.push(Signal::new(
            format!("unsettled {key}"),
            "act",
            "a batch run of the API credit is still running more than 26 hours after its start; it counts at its reservation until it is closed",
            format!("run {key}, job {job}, started_at {started}, reserved_usd_micros {reserved}"),
        ));
    }
}

/// `key mode`: one `stat` of the key file, a link not followed; never a read of its bytes.
fn key_mode(s: &mut impl System, path: &str, out: &mut Vec<Signal>) -> Result<(), String> {
    let words = ["-c".to_owned(), "%a %F".to_owned(), path.to_owned()];
    let reply = s
        .call("stat", &words, &Value::Null, MS)
        .map_err(|_| "stat of the key file could not run".to_owned())?;
    if !reply.success {
        let said = String::from_utf8_lossy(&reply.bytes);
        let absent = ["No such file or directory", "Not a directory"];
        return match absent.iter().any(|a| said.contains(a)) {
            true => Ok(()),
            false => Err("stat of the key file failed".to_owned()),
        };
    }
    let text = String::from_utf8_lossy(&reply.stdout);
    let (mode, kind) = text
        .trim()
        .split_once(' ')
        .filter(|(m, k)| !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit()) && !k.is_empty())
        .ok_or_else(|| "stat of the key file printed no mode and type".to_owned())?;
    if mode != "600" || !regular(kind) {
        out.push(Signal::new(
            "key mode".to_owned(),
            "urgent",
            "the key file of the API credit is not a regular file of mode 600",
            format!("mode {mode}, type {kind}"),
        ));
    }
    Ok(())
}

/// `credit` (R31).
pub(crate) fn detect(
    s: &mut impl System,
    now: u64,
    policy: &Result<Option<Value>, String>,
) -> Reading {
    let unknown = || Read {
        signals: vec![],
        unknown: FAMILIES.iter().map(|f| (*f).to_owned()).collect(),
        state: None,
    };
    let policy = match policy {
        Ok(Some(p)) => p,
        Ok(None) => return Ok(unknown()),
        Err(cause) => return Err(cause.clone()),
    };
    let api = match config(policy) {
        Ok(Some(api)) => api,
        Ok(None) => return Ok(unknown()),
        Err(f) => {
            let detail = f.detail.as_str().unwrap_or_default();
            return Err(format!("policy api is invalid: {detail}"));
        }
    };
    let all = runs(s)?;
    let mut r = Read::default();
    stops(
        now,
        &all,
        [("standard", api.standard), ("soak", api.soak)],
        &mut r.signals,
    );
    unsettled(now, &all, &mut r.signals);
    key_mode(s, &api.key_file, &mut r.signals)?;
    Ok(r)
}
