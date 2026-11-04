use crate::Result;
use chrono::{SecondsFormat, Utc};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};
pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
pub fn seconds(value: &str) -> f64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|x| x.timestamp_millis() as f64 / 1000.)
        .unwrap_or(f64::NAN)
}
pub fn hash(value: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(value.as_ref()))
}
pub fn id() -> String {
    let mut b = [0; 16];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .expect("OS random source unavailable");
    b[6] = (b[6] & 15) | 64;
    b[8] = (b[8] & 63) | 128;
    let h = b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}
pub fn matches(pattern: &str, value: &str) -> bool {
    regex::Regex::new(pattern)
        .expect("constant regex")
        .is_match(value)
}
pub fn sha(value: &str) -> bool {
    matches("^[a-f0-9]{40}$", value)
}
pub fn digest(value: &str) -> bool {
    matches("^[a-f0-9]{64}$", value)
}
pub fn run_id(value: &str) -> bool {
    matches("^[A-Za-z0-9-]{16,128}$", value)
}
pub fn json<T: DeserializeOwned>(p: impl AsRef<Path>) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(p)?)?)
}
pub fn bounded(p: impl AsRef<Path>, max: u64) -> Result<Vec<u8>> {
    let p = p.as_ref();
    let m = fs::symlink_metadata(p)?;
    if !m.is_file() || m.len() > max {
        return Err("Expected bounded regular file".into());
    }
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(0x20000 | 0x800)
        .open(p)?;
    let m2 = f.metadata()?;
    if !m2.is_file() || m2.len() > max {
        return Err("Expected bounded regular file".into());
    }
    let mut b = Vec::new();
    std::io::Read::by_ref(&mut f)
        .take(max + 1)
        .read_to_end(&mut b)?;
    if b.len() as u64 > max {
        return Err("File exceeds bound".into());
    }
    Ok(b)
}
pub fn sync_dir(p: &Path) -> Result<()> {
    File::open(p)?.sync_all()?;
    Ok(())
}
pub fn write_new(p: &Path, b: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(p)?;
    f.write_all(b)?;
    f.sync_all()?;
    Ok(())
}
pub fn bytes<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    let mut b = serde_json::to_vec_pretty(v)?;
    b.push(b'\n');
    Ok(b)
}
pub fn save(p: &Path, v: &impl Serialize) -> Result<()> {
    fs::create_dir_all(p.parent().ok_or("Missing parent")?)?;
    let temp = p.with_extension(format!("{}.tmp", id()));
    write_new(&temp, &bytes(v)?)?;
    fs::rename(&temp, p)?;
    sync_dir(p.parent().ok_or("Missing parent")?)
}
pub fn record(p: &Path, v: &impl Serialize) -> Result<()> {
    fs::create_dir_all(p.parent().ok_or("Missing parent")?)?;
    write_new(p, &bytes(v)?)?;
    sync_dir(p.parent().ok_or("Missing parent")?)
}
pub fn immutable(p: &Path, v: &impl Serialize) -> Result<()> {
    fs::create_dir_all(p.parent().ok_or("Missing parent")?)?;
    let temp = p.with_extension(format!("{}.tmp", id()));
    write_new(&temp, &bytes(v)?)?;
    fs::hard_link(&temp, p)?;
    sync_dir(p.parent().ok_or("Missing parent")?)?;
    fs::remove_file(temp)?;
    Ok(())
}
pub fn names(p: &Path) -> Result<Vec<String>> {
    let mut n = fs::read_dir(p)?
        .map(|e| Ok(e?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>>>()?;
    n.sort();
    Ok(n)
}
pub fn literal(v: &impl Serialize) -> Result<String> {
    Ok(format!(
        "json.loads({})",
        serde_json::to_string(&serde_json::to_string(v)?)?
    ))
}
pub fn ps_quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', "''"))
}
pub fn windows_path(v: &str) -> String {
    v.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}
pub fn utf16_base64(v: &str) -> String {
    use base64::Engine;
    let b: Vec<u8> = v.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// Python's single-line JSON spacing is retained for the installed canary CLI.
pub fn python_json(v: &serde_json::Value, sorted: bool) -> Result<String> {
    let mut value = v.clone();
    if sorted {
        value.sort_all_objects();
    }
    let compact = serde_json::to_string(&value)?;
    let mut result = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in compact.chars() {
        result.push(c);
        if quoted {
            if escaped {
                escaped = false
            } else if c == '\\' {
                escaped = true
            } else if c == '"' {
                quoted = false
            }
        } else if c == '"' {
            quoted = true
        } else if c == ',' || c == ':' {
            result.push(' ')
        }
    }
    Ok(result)
}

/// Match JSON.parse/JSON.stringify integer normalization at the persisted JS boundary.
pub fn normalize_integers(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Number(n) => {
            if n.is_f64()
                && let Some(f) = n.as_f64()
                && f >= 0.
                && f.fract() == 0.
                && f <= 9007199254740991.
            {
                *n = serde_json::Number::from(f as u64);
            }
        }
        serde_json::Value::Array(a) => {
            for x in a {
                normalize_integers(x)
            }
        }
        serde_json::Value::Object(o) => {
            for x in o.values_mut() {
                normalize_integers(x)
            }
        }
        _ => {}
    }
}
