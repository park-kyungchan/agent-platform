//! Disposable fixture adapter; no environment override exists in `ap pane`.
use ap::pane::{Failure, System};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, VecDeque},
    io::{self, Read},
    path::{Path, PathBuf},
};

struct Fixture {
    value: Value,
    queries: VecDeque<Value>,
}
impl Fixture {
    fn new(value: Value) -> Self {
        Self {
            queries: value["queries"].as_array().unwrap().clone().into(),
            value,
        }
    }
}
impl System for Fixture {
    fn query(&mut self, args: &[&str]) -> Result<Value, Failure> {
        let next = self.queries.pop_front().expect("unexpected query");
        assert_eq!(serde_json::to_value(args).unwrap(), next["args"]);
        if let Some(kind) = next["error"].as_str() {
            return Err(Failure(
                "herdr_query_failed",
                if kind == "status" {
                    "could not query the current Herdr session"
                } else {
                    "could not parse the current Herdr session"
                }
                .into(),
            ));
        }
        Ok(next["result"].clone())
    }
    fn proc_stat(&self, pid: i64) -> Option<String> {
        std::fs::read_to_string(
            Path::new(self.value["proc_root"].as_str().unwrap()).join(format!("{pid}/stat")),
        )
        .ok()
    }
    fn pid(&self) -> i64 {
        900
    }
    fn session_id(&self) -> String {
        self.value["session"].as_str().unwrap_or("").into()
    }
    fn origin(&self, _: &str) -> Option<(i64, String)> {
        let a = self.value["origin"].as_array()?;
        Some((a[0].as_i64()?, a[1].as_str()?.into()))
    }
    fn boot_and_ticks(&self) -> Option<(i64, u64)> {
        Some((1000, 100))
    }
    fn caller_ttys(&self) -> BTreeSet<u64> {
        self.value["ttys"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default()
    }
    fn shell_tty(&self, pid: i64) -> Option<u64> {
        self.value["shell_ttys"][pid.to_string()].as_u64()
    }
    fn realpath(&self, p: &str) -> PathBuf {
        PathBuf::from(p)
    }
}
fn run(value: Value) -> Value {
    let args = value["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let env = value["herdr_env"].as_bool().unwrap_or(true);
    let mut fixture = Fixture::new(value);
    let result = match ap::pane::resolve(&mut fixture, &args, env) {
        Ok((stdout, stderr)) => json!({"stdout":stdout,"stderr":stderr,"status":0}),
        Err(e) => json!({"stdout":"","stderr":e.stderr(),"status":1}),
    };
    assert!(fixture.queries.is_empty(), "unconsumed query fixture");
    result
}
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    println!("{}", run(serde_json::from_str(&input).unwrap()));
}
