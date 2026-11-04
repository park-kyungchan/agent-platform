use super::workflow::Checkpoint;
use crate::{Result, util};
use std::{fs, path::Path};
pub fn save(dir: &Path, s: &Checkpoint) -> Result<()> {
    let h = dir.join("history");
    fs::create_dir_all(&h)?;
    util::immutable(&h.join(format!("{:06}.json", s.revision)), s)?;
    util::save(&dir.join("state.json"), s)
}
pub fn load(dir: &Path) -> Result<Checkpoint> {
    let h = dir.join("history");
    let n = util::names(&h)?
        .into_iter()
        .rfind(|s| util::matches("^[0-9]{6}\\.json$", s))
        .ok_or("Update checkpoint history is empty")?;
    let s: Checkpoint = util::json(h.join(&n))?;
    if s.schema_version != 1
        || s.revision == 0
        || s.revision != n[..6].parse::<u64>()?
        || s.target_sha.as_deref().is_some_and(|x| !util::sha(x))
    {
        return Err("Invalid update checkpoint".into());
    }
    Ok(s)
}
