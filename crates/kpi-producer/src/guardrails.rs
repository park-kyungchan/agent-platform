//! Strict sanitized PR5 recordings. Source-qualified values are distinct from bounds.
use crate::{Error, Result, hex, model::*, strict, text};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
const WEEK: u64 = 604800;
pub const RECORDING_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Month {
    pub year: u16,
    pub month: u8,
}
impl Month {
    pub fn bounds(&self) -> Result<(u64, u64)> {
        if !(1970..=9999).contains(&self.year) || !(1..=12).contains(&self.month) {
            return Err(Error("invalid_month"));
        }
        fn leap(y: u64) -> bool {
            y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400))
        }
        let mut days = 0u64;
        for y in 1970..u64::from(self.year) {
            days += if leap(y) { 366 } else { 365 };
        }
        let lengths = [
            31,
            if leap(self.year.into()) { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        for n in lengths.iter().take(usize::from(self.month - 1)) {
            days += n;
        }
        Ok((
            days * 86400,
            (days + lengths[usize::from(self.month - 1)]) * 86400,
        ))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskUnit {
    Bytes,
    Gb,
    Gib,
}
impl DiskUnit {
    fn multiplier(&self) -> u64 {
        match self {
            Self::Bytes => 1,
            Self::Gb => 1_000_000_000,
            Self::Gib => GIB,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiskMinimum {
    Exact { quantity: u64, unit: DiskUnit },
    UnresolvedGbOrGib { quantity: u64 },
}
impl DiskMinimum {
    pub fn bounds(&self) -> Result<(u64, u64)> {
        let mul = |a: u64, b: u64| a.checked_mul(b).ok_or(Error("invalid_disk_contract"));
        match self {
            Self::Exact { quantity, unit } => {
                let n = mul(*quantity, unit.multiplier())?;
                Ok((n, n))
            }
            Self::UnresolvedGbOrGib { quantity } => {
                Ok((mul(*quantity, 1_000_000_000)?, mul(*quantity, GIB)?))
            }
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub supported: u64,
    pub total: u64,
    pub evaluation_set_sha256: String,
    pub result_revision: String,
    pub artifact_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Contract {
    Billing {
        account: String,
        month: Month,
        linux_skus: BTreeSet<String>,
        limit: u64,
    },
    Disk {
        minimum: DiskMinimum,
    },
    Recall {
        baseline: Baseline,
    },
    Defects {
        week_start: u64,
        limit: u64,
    },
}
impl Contract {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Billing { .. } => "billing",
            Self::Disk { .. } => "disk",
            Self::Recall { .. } => "recall",
            Self::Defects { .. } => "defects",
        }
    }
    pub fn unit(&self) -> &'static str {
        match self {
            Self::Billing { .. } => "linux_minutes",
            Self::Disk { .. } => "bytes",
            Self::Recall { .. } => "fraction",
            Self::Defects { .. } => "count",
        }
    }
    pub fn budget(&self) -> u64 {
        match self {
            Self::Billing { .. } => 86400,
            Self::Disk { .. } => 900,
            _ => 3600,
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Billing {
                account,
                month,
                linux_skus,
                ..
            } => {
                month.bounds()?;
                if !text(account)
                    || linux_skus.is_empty()
                    || linux_skus.len() > 64
                    || linux_skus.iter().any(|s| !text(s))
                {
                    return Err(Error("invalid_billing_contract"));
                }
            }
            Self::Disk { minimum } => {
                minimum.bounds()?;
            }
            Self::Recall { baseline: b } => {
                if b.total == 0
                    || b.supported > b.total
                    || !hex(&b.evaluation_set_sha256, 64)
                    || !hex(&b.artifact_sha256, 64)
                    || !text(&b.result_revision)
                {
                    return Err(Error("invalid_recall_contract"));
                }
            }
            Self::Defects { week_start, .. } => {
                if *week_start % 86400 != 0
                    || (*week_start / 86400 + 3) % 7 != 0
                    || week_start.checked_add(WEEK).is_none()
                {
                    return Err(Error("invalid_week_contract"));
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recording {
    pub source_revision: String,
    pub capture_start: u64,
    pub capture_end: u64,
    pub outcome: Outcome,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    NotPerformed {},
    Failed {},
    Baseline {
        observed_at: u64,
        value: u64,
        total: Option<u64>,
    },
    Billing {
        account: String,
        month: Month,
        period_start: u64,
        period_end: u64,
        cutoff: u64,
        settlement_lag_seconds: Option<u64>,
        population_complete: bool,
        pages: Vec<Page>,
    },
    Disk {
        observed_at: u64,
        numerator: u64,
        denominator: u64,
        unit: DiskUnit,
    },
    Recall {
        observed_at: u64,
        evaluation_set_sha256: String,
        result_revision: String,
        supported: u64,
        total: u64,
    },
    Defects {
        week_start: u64,
        week_end: u64,
        cutoff: u64,
        population_complete: bool,
        incidents: Vec<Incident>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub number: u32,
    pub has_more: bool,
    pub items: Vec<Usage>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub id: String,
    pub sku: String,
    pub unit: String,
    pub quantity: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Incident {
    pub id: String,
    pub escaped_at: u64,
    pub fixed_at: Option<u64>,
}
#[derive(Debug)]
pub struct Evaluation {
    pub value: K<u64>,
    pub total: K<u64>,
    pub observed_at: K<u64>,
    pub qualification: Qualification,
    pub source_qualified: bool,
}
impl Evaluation {
    fn unresolved(q: Qualification) -> Self {
        Self {
            value: if q == Qualification::NotPerformed {
                K::NotPerformed
            } else {
                K::Unknown
            },
            total: if q == Qualification::NotPerformed {
                K::NotPerformed
            } else {
                K::Unknown
            },
            observed_at: K::Unknown,
            qualification: q,
            source_qualified: false,
        }
    }
}
pub fn evaluate(bytes: Option<&[u8]>, contract: &Contract, now: u64) -> Result<Evaluation> {
    contract.validate()?;
    let Some(bytes) = bytes else {
        return Ok(Evaluation::unresolved(Qualification::MissingSource));
    };
    let r: Recording = match strict::parse(bytes, RECORDING_LIMIT) {
        Ok(r) => r,
        Err(_) => return Ok(Evaluation::unresolved(Qualification::InvalidSource)),
    };
    let mut e = match evaluate_inner(&r, contract) {
        Ok(e) => e,
        Err(_) => return Ok(Evaluation::unresolved(Qualification::InvalidSource)),
    };
    if !text(&r.source_revision) || r.capture_start > r.capture_end {
        return Ok(Evaluation::unresolved(Qualification::InvalidSource));
    }
    if r.capture_end > now || e.observed_at.get().is_some_and(|at| *at > r.capture_end) {
        return Ok(Evaluation::unresolved(Qualification::FutureSource));
    }
    if e.source_qualified
        && e.observed_at
            .get()
            .is_none_or(|at| now.saturating_sub(*at) > contract.budget())
    {
        let at = e.observed_at;
        e = Evaluation::unresolved(Qualification::StaleSource);
        e.observed_at = at;
    }
    Ok(e)
}
fn evaluate_inner(r: &Recording, c: &Contract) -> Result<Evaluation> {
    let mut e = Evaluation::unresolved(Qualification::InvalidSource);
    match (&r.outcome, c) {
        (Outcome::NotPerformed {}, _) => {
            return Ok(Evaluation::unresolved(Qualification::NotPerformed));
        }
        (Outcome::Failed {}, _) => return Ok(Evaluation::unresolved(Qualification::FailedSource)),
        (
            Outcome::Baseline {
                observed_at,
                value,
                total,
            },
            _,
        ) => {
            if total.is_some_and(|t| t == 0 || *value > t) {
                return Err(Error("invalid_baseline"));
            }
            e.qualification = Qualification::UnverifiedBaseline;
            e.observed_at = K::Value(*observed_at);
            return Ok(e);
        }
        (
            Outcome::Billing {
                account,
                month,
                period_start,
                period_end,
                cutoff,
                settlement_lag_seconds,
                population_complete,
                pages,
            },
            Contract::Billing {
                account: expected,
                month: expected_month,
                linux_skus,
                ..
            },
        ) => {
            if account != expected
                || month != expected_month
                || (*period_start, *period_end) != month.bounds()?
                || cutoff < period_start
                || cutoff > period_end
                || pages.len() > 128
            {
                return Err(Error("invalid_billing_source"));
            }
            e.observed_at = K::Value(*cutoff);
            let mut sum = 0u64;
            let mut ids = BTreeSet::new();
            let mut complete = *population_complete && !pages.is_empty();
            for (i, p) in pages.iter().enumerate() {
                if p.number as usize != i + 1
                    || p.items.len() > 4096
                    || (i + 1 < pages.len() && !p.has_more)
                {
                    return Err(Error("invalid_billing_pages"));
                }
                for item in &p.items {
                    if !text(&item.id)
                        || !ids.insert(&item.id)
                        || !linux_skus.contains(&item.sku)
                        || item.unit != "linux_minutes"
                    {
                        return Err(Error("invalid_billing_item"));
                    }
                    sum = sum
                        .checked_add(item.quantity)
                        .ok_or(Error("billing_overflow"))?;
                }
                if i + 1 == pages.len() && p.has_more {
                    complete = false;
                }
            }
            if !complete {
                e.qualification = Qualification::IncompleteSource;
                return Ok(e);
            }
            if *settlement_lag_seconds != Some(0) {
                e.qualification = Qualification::SettlementLag;
                return Ok(e);
            }
            e.value = K::Value(sum);
        }
        (
            Outcome::Disk {
                observed_at,
                numerator,
                denominator,
                unit,
            },
            Contract::Disk { minimum },
        ) => {
            if *denominator == 0 {
                return Err(Error("invalid_disk_source"));
            }
            let scaled = u128::from(*numerator) * u128::from(unit.multiplier());
            if scaled % u128::from(*denominator) != 0 {
                return Err(Error("inexact_disk_source"));
            }
            e.value = K::Value(
                u64::try_from(scaled / u128::from(*denominator))
                    .map_err(|_| Error("disk_overflow"))?,
            );
            e.observed_at = K::Value(*observed_at);
            let (lo, hi) = minimum.bounds()?;
            if lo != hi {
                e.qualification = Qualification::UnresolvedDiskUnit;
            }
        }
        (
            Outcome::Recall {
                observed_at,
                evaluation_set_sha256,
                result_revision,
                supported,
                total,
            },
            Contract::Recall { baseline },
        ) => {
            if *total == 0
                || supported > total
                || *total != baseline.total
                || evaluation_set_sha256 != &baseline.evaluation_set_sha256
                || !text(result_revision)
            {
                return Err(Error("invalid_recall_source"));
            }
            e.value = K::Value(*supported);
            e.total = K::Value(*total);
            e.observed_at = K::Value(*observed_at);
        }
        (
            Outcome::Defects {
                week_start,
                week_end,
                cutoff,
                population_complete,
                incidents,
            },
            Contract::Defects {
                week_start: expected,
                ..
            },
        ) => {
            if week_start != expected
                || *week_end != week_start.checked_add(WEEK).ok_or(Error("invalid_week"))?
                || cutoff < week_start
                || cutoff > week_end
                || incidents.len() > 4096
            {
                return Err(Error("invalid_defect_source"));
            }
            e.observed_at = K::Value(*cutoff);
            let mut ids = BTreeSet::new();
            for i in incidents {
                if !text(&i.id)
                    || !ids.insert(&i.id)
                    || i.escaped_at < *week_start
                    || i.escaped_at >= *cutoff
                    || i.fixed_at
                        .is_some_and(|at| at < i.escaped_at || at > *cutoff)
                {
                    return Err(Error("invalid_incident"));
                }
            }
            if !population_complete {
                e.qualification = Qualification::IncompleteSource;
                return Ok(e);
            }
            e.value = K::Value(incidents.len() as u64);
        }
        _ => return Err(Error("recording_kind_mismatch")),
    }
    e.source_qualified = true;
    if e.qualification != Qualification::UnresolvedDiskUnit {
        e.qualification = Qualification::Qualified;
    }
    Ok(e)
}
