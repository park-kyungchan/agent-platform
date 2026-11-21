//! The principal `agent`: one grant view of the installed schemas, from
//! which both the admission `Policy` and `describe` are derived, so the
//! two cannot differ. Also the roles of a call (`specs/ont/action.md` R112
//! to R114): the roles file beside the store, read at most once per call,
//! and the grants that apply to a call's resolved role.
use crate::{Failure, Refusal, internal};
use ont_core::schema::{Schema, SchemaGrant, SchemaSet};
use ont_core::{Authority, Grant, Policy, Scope, Session};
use serde::Deserialize;
use serde_json::json;
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const PRINCIPAL: &str = "agent";

/// The kernel file that holds the node name of the reader's UTS namespace
/// (what `uname -n` prints).
const NODE_NAME_FILE: &str = "/proc/sys/kernel/hostname";

/// Who the engine is for one call: the OS account and the node it runs on,
/// as the kernel reports them. Nothing the caller supplies reaches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineIdentity {
    /// `uid:<n>`, n the real uid of this process.
    pub principal: String,
    /// The node name of this process, without a line end.
    pub host: String,
}

/// Read the engine's identity from the kernel, now. Fails (internal,
/// `actor_identity_unreadable`) when the node name cannot be read, is empty
/// or is not UTF-8. This is the one place that reads it; it takes no
/// argument, so no request, option, environment variable or `Context` member
/// can set either value.
pub fn engine_identity() -> Result<EngineIdentity, Failure> {
    let unreadable = || Failure::Internal(internal!("actor_identity_unreadable"));
    let uid = rustix::process::getuid().as_raw();
    let bytes = std::fs::read(NODE_NAME_FILE).map_err(|_| unreadable())?;
    let text = String::from_utf8(bytes).map_err(|_| unreadable())?;
    let host = text.strip_suffix('\n').unwrap_or(&text);
    if host.is_empty() {
        return Err(unreadable());
    }
    Ok(EngineIdentity {
        principal: format!("uid:{uid}"),
        host: host.to_string(),
    })
}

/// What one grant of the principal allows, with its scope built.
#[derive(Debug, Clone)]
pub struct GrantEntry {
    pub domain: String,
    pub scope: Scope,
    pub read: BTreeSet<String>,
    pub derive: BTreeSet<String>,
    pub actions: BTreeSet<String>,
    /// The grant's role: `None` for a grant of every call.
    pub role: Option<String>,
    /// The functions the grant lets a call run.
    pub functions: BTreeSet<String>,
}

/// The principal's grants over all installed schemas.
#[derive(Debug, Clone, Default)]
pub struct GrantView {
    pub entries: Vec<GrantEntry>,
}
impl GrantView {
    /// Every grant of the principal, with or without a role. Fails
    /// (internal) on a grant whose scope cannot be built: schema check
    /// refuses such a grant, so an installed schema never has one.
    pub fn from_set(set: &SchemaSet) -> Result<Self, Failure> {
        Self::build(set, |_| true)
    }
    /// The grants that apply to a call whose resolved role is `role`
    /// (`specs/ont/action.md` R114): in every installed domain, the grants
    /// without a role and, when `role` is given, the grants of that role.
    pub fn for_role(set: &SchemaSet, role: Option<&str>) -> Result<Self, Failure> {
        Self::build(set, |g| {
            g.role
                .as_deref()
                .is_none_or(|r| role.is_some_and(|c| c == r))
        })
    }
    fn build(set: &SchemaSet, keep: impl Fn(&SchemaGrant) -> bool) -> Result<Self, Failure> {
        let mut entries = Vec::new();
        for domain in set.pins().keys() {
            let Some(schema) = set.schema(domain) else {
                continue;
            };
            for g in schema
                .grants
                .iter()
                .filter(|g| g.principal == PRINCIPAL && keep(g))
            {
                let Ok(scope) = Scope::domain(
                    schema.namespace.clone(),
                    schema.domain.clone(),
                    g.scope.clone(),
                ) else {
                    return Err(Failure::Internal(internal!("grant_scope_invalid")));
                };
                entries.push(GrantEntry {
                    domain: schema.domain.clone(),
                    scope,
                    read: g.read.iter().cloned().collect(),
                    derive: g.derive.iter().cloned().collect(),
                    actions: g.actions.iter().cloned().collect(),
                    role: g.role.clone(),
                    functions: g.functions.iter().cloned().collect(),
                });
            }
        }
        Ok(Self { entries })
    }
    /// Object types of `domain` the principal may read.
    pub fn read_types(&self, domain: &str) -> BTreeSet<String> {
        self.entries
            .iter()
            .filter(|e| e.domain == domain)
            .flat_map(|e| e.read.iter().cloned())
            .collect()
    }
    /// Action types of `domain` the principal may apply.
    pub fn action_types(&self, domain: &str) -> BTreeSet<String> {
        self.entries
            .iter()
            .filter(|e| e.domain == domain)
            .flat_map(|e| e.actions.iter().cloned())
            .collect()
    }
    /// Functions of `domain` the grants let a call run.
    pub fn functions(&self, domain: &str) -> BTreeSet<String> {
        self.entries
            .iter()
            .filter(|e| e.domain == domain)
            .flat_map(|e| e.functions.iter().cloned())
            .collect()
    }
    /// Object types readable in any domain.
    pub fn all_read_types(&self) -> BTreeSet<String> {
        self.entries
            .iter()
            .flat_map(|e| e.read.iter().cloned())
            .collect()
    }
    /// Domains with at least one readable object type, applicable action or
    /// function a grant lists.
    pub fn domains(&self) -> BTreeSet<String> {
        self.entries
            .iter()
            .filter(|e| !e.read.is_empty() || !e.actions.is_empty() || !e.functions.is_empty())
            .map(|e| e.domain.clone())
            .collect()
    }
    /// The admission policy. Each read/derive type gets its own grant so
    /// neither flag leaks to the other.
    pub(crate) fn policy(&self) -> Policy {
        let mut grants: Vec<Grant> = Vec::new();
        for e in &self.entries {
            let types: BTreeSet<&String> = e.read.iter().chain(e.derive.iter()).collect();
            for t in types {
                grants.push(Grant {
                    scope: e.scope.clone(),
                    object_types: BTreeSet::from([t.clone()]),
                    read: e.read.contains(t),
                    derive: e.derive.contains(t),
                    actions: e.actions.iter().cloned().collect(),
                });
            }
        }
        Policy {
            principals: BTreeMap::from([(PRINCIPAL.to_string(), grants)]),
        }
    }
}

/// The policy generation of a fresh `Authority`, read from the `Authority`
/// (never a literal). Operations that bind no principal (`backup`, `index`,
/// `schema install`) take their `authorization_revision` from here.
pub fn fresh_revision() -> Result<u64, Failure> {
    Authority::new(Policy::default())
        .generation()
        .map_err(|_| Failure::Internal(internal!("authority_failure")))
}

/// The bound principal of one request.
pub struct Principal {
    authority: Authority,
    session: Session,
    view: GrantView,
}
impl Principal {
    /// Build the `Authority` from every installed grant of `agent`, with or
    /// without a role, and bind `agent` through `bind_trusted` (the caller
    /// checked uid 0 first). The reads and `action list` take no role and
    /// use this binding.
    pub fn bind(set: &SchemaSet) -> Result<Self, Failure> {
        Self::with_view(GrantView::from_set(set)?)
    }
    /// The binding of a call of `action validate`, `action apply` or `fn`
    /// whose resolved role is `role` (`specs/ont/action.md` R114): only the
    /// grants that apply to that role.
    pub fn bind_role(set: &SchemaSet, role: Option<&str>) -> Result<Self, Failure> {
        Self::with_view(GrantView::for_role(set, role)?)
    }
    fn with_view(view: GrantView) -> Result<Self, Failure> {
        let authority = Authority::new(view.policy());
        let session = authority
            .bind_trusted(PRINCIPAL)
            .map_err(|_| Failure::Internal(internal!("authority_failure")))?;
        Ok(Self {
            authority,
            session,
            view,
        })
    }
    pub fn view(&self) -> &GrantView {
        &self.view
    }
    pub fn authority(&self) -> &Authority {
        &self.authority
    }
    pub fn session(&self) -> &Session {
        &self.session
    }
    /// `authorization_revision` of an answer: the policy generation of the
    /// `Authority` built for this request, read through
    /// `Authority::generation` (a poisoned lock is `authority_failure`).
    pub fn revision(&self) -> Result<u64, Failure> {
        self.authority
            .generation()
            .map_err(|_| Failure::Internal(internal!("authority_failure")))
    }
}

/// The roles file, beside the store (`specs/ont/action.md` R113).
pub const ROLES_FILE: &str = "roles.json";
/// The largest roles file that is usable.
pub const ROLES_FILE_MAX_BYTES: usize = 65_536;

/// The roles file of the store at `store`.
pub fn roles_path(store: &Path) -> PathBuf {
    match store.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(ROLES_FILE),
        _ => PathBuf::from(ROLES_FILE),
    }
}

/// Whether the domain of `schema` is role-checked
/// (`specs/ont/schema-language.md` R30): a grant of the schema names a role
/// or lists a function.
pub fn role_checked(schema: &Schema) -> bool {
    schema
        .grants
        .iter()
        .any(|g| g.role.is_some() || !g.functions.is_empty())
}

/// What the roles file says of a principal and a claimed role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoleAdmission {
    Admitted,
    /// No file there.
    RolesFileAbsent,
    /// Any other fault of the file or of its form.
    RolesFileUnusable,
    /// The file does not list the role for the principal.
    RoleNotListed,
}
impl RoleAdmission {
    /// The `detail` of `role_not_admitted`; `None` when admitted.
    pub(crate) fn detail(self) -> Option<&'static str> {
        match self {
            Self::Admitted => None,
            Self::RolesFileAbsent => Some("roles_file_absent"),
            Self::RolesFileUnusable => Some("roles_file_unusable"),
            Self::RoleNotListed => Some("role_not_listed"),
        }
    }
}

/// The form of the roles file: `{"principals":{"uid:<n>":[role, ...]}}`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RolesFile {
    principals: UniqueMap,
}
/// An object whose names occur once each.
struct UniqueMap(BTreeMap<String, Vec<String>>);
impl<'de> Deserialize<'de> for UniqueMap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = UniqueMap;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object of arrays of role names")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut m: A,
            ) -> Result<UniqueMap, A::Error> {
                let mut out = BTreeMap::new();
                while let Some((k, v)) = m.next_entry::<String, Vec<String>>()? {
                    if out.insert(k, v).is_some() {
                        return Err(serde::de::Error::custom("duplicate principal"));
                    }
                }
                Ok(UniqueMap(out))
            }
        }
        d.deserialize_map(Visit)
    }
}

/// A principal as the engine writes it (`specs/ont/action.md` R101).
fn principal_form(s: &str) -> bool {
    s.strip_prefix("uid:")
        .is_some_and(|n| !n.is_empty() && n.len() <= 10 && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Read the roles file of `store` for `principal` and `role`, now
/// (`specs/ont/action.md` R113). The file is opened without following a
/// symbolic link and without blocking; that it is a regular file, its owner
/// (uid 0 or the engine's real uid) and its mode (no write bit for group or
/// others) are read from the opened descriptor, and at most
/// `ROLES_FILE_MAX_BYTES` + 1 bytes are read. Nothing is created or written.
pub(crate) fn roles_file_admits(store: &Path, principal: &str, role: &str) -> RoleAdmission {
    use rustix::fs::{FileType, Mode, OFlags};
    let unusable = RoleAdmission::RolesFileUnusable;
    let fd = match rustix::fs::open(
        roles_path(store),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return RoleAdmission::RolesFileAbsent,
        Err(_) => return unusable,
    };
    let Ok(stat) = rustix::fs::fstat(&fd) else {
        return unusable;
    };
    let real_uid = rustix::process::getuid().as_raw();
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || (stat.st_uid != 0 && stat.st_uid != real_uid)
        || stat.st_mode & 0o022 != 0
    {
        return unusable;
    }
    let mut bytes = Vec::new();
    if std::fs::File::from(fd)
        .take(ROLES_FILE_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() > ROLES_FILE_MAX_BYTES
    {
        return unusable;
    }
    let Ok(file) = serde_json::from_slice::<RolesFile>(&bytes) else {
        return unusable;
    };
    let principals = file.principals.0;
    if principals.iter().any(|(p, roles)| {
        !principal_form(p) || roles.iter().any(|r| !crate::action::valid_actor_field(r))
    }) {
        return unusable;
    }
    match principals.get(principal) {
        Some(roles) if roles.iter().any(|r| r == role) => RoleAdmission::Admitted,
        _ => RoleAdmission::RoleNotListed,
    }
}

/// The refusal of a claimed role the roles file does not admit; the claimed
/// value is not in it.
pub(crate) fn role_not_admitted(detail: &'static str) -> Refusal {
    Refusal::new("role_not_admitted")
        .with("field", json!("/role"))
        .with("detail", json!(detail))
}

/// The refusal of an action or a function outside the grants that apply in
/// a role-checked domain.
pub(crate) fn not_granted() -> Refusal {
    Refusal::new("not_granted")
}

/// The role of one call (`specs/ont/action.md` R112): the claimed role,
/// checked against the roles file for the engine's principal when the
/// domain found is role-checked. The file is read at most once per call
/// (the first resolution in a role-checked domain); nothing is kept between
/// calls.
pub(crate) struct RoleResolver<'a> {
    store: &'a Path,
    principal: &'a str,
    claimed: Option<&'a str>,
    read: OnceCell<RoleAdmission>,
}
impl<'a> RoleResolver<'a> {
    /// `principal` is the one `engine_identity` read for this call;
    /// `claimed` the call's claimed role, already within its bound.
    pub(crate) fn new(store: &'a Path, principal: &'a str, claimed: Option<&'a str>) -> Self {
        Self {
            store,
            principal,
            claimed,
            read: OnceCell::new(),
        }
    }
    /// The resolved role of a call found in the domain of `schema`: `None`
    /// when the domain is not role-checked or no role is claimed, the
    /// claimed role when the file admits it, else `role_not_admitted`.
    pub(crate) fn resolve(&self, schema: &Schema) -> Result<Option<&'a str>, Refusal> {
        if !role_checked(schema) {
            return Ok(None);
        }
        let Some(role) = self.claimed else {
            return Ok(None);
        };
        let admission = *self
            .read
            .get_or_init(|| roles_file_admits(self.store, self.principal, role));
        match admission.detail() {
            None => Ok(Some(role)),
            Some(detail) => Err(role_not_admitted(detail)),
        }
    }
}
