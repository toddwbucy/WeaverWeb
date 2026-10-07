//! conforms: web-position-is-stored-at-ingest
//! conforms: web-record-members-agree-across-a-run
//! conforms: web-record-members-are-absent-where-unsent
//!
//! **One `weaver-analysis signals` emission, read and planned**, the pure
//! half of the ingest of `weaver-web-Spec` section 3.1: the wire's two
//! streams (`weaver-analysis-web-contract` sections 2.1 and 2.2) parsed and
//! checked against each other, and each run the emission carries turned into
//! the rows it lands as, or into the reason it is refused. Nothing here
//! touches the store; `ingest.rs` writes what this plans.
//!
//! **Every refusal this file makes is made before a row exists**, so it is
//! the answer's alone and the store never holds a row for a run refused
//! here (the rulings of 2026-10-06, the brief's last paragraph on 10b's
//! choices): a run whose generations disagree, that names no effective
//! sampling, that carries no weights hash, whose positions cannot be formed,
//! or that names one key twice with two payloads.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::io::BufRead;

pub use super::rows::{GenerationRow, KEY_BOUND, PositionRow, RunMembers};

/// The longest line read, in bytes, summary or point. A point is a few
/// dozen bytes; the summary grows with the generations it lists, and a
/// record of thousands of generations stays far under this. A line past it
/// refuses the emission rather than growing a buffer without end.
pub const LINE_BOUND: usize = 16 * 1024 * 1024;

/// **The most positions a summary may announce.** The whole emission is
/// held in memory, since a run is planned whole before any row is written,
/// so its size needs a bound and not only its lines. This is an elected
/// figure with headroom and not a measurement: thirty generations each
/// filling a 131,072-token context is about four million positions, past
/// any trace this repository's fixtures hold, and the deposits that might
/// hold a larger one are not in this repository. A later act may raise it.
pub const POSITIONS_BOUND: usize = 4_000_000;

/// **The most bytes an emission may carry**, every line counted. At the
/// emitter's few dozen bytes a point, `POSITIONS_BOUND` points sit well
/// inside it.
pub const EMISSION_BOUND: u64 = 1024 * 1024 * 1024;

/// The reader's bounds, the documented ones unless a test names others.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub positions: usize,
    pub bytes: u64,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            positions: POSITIONS_BOUND,
            bytes: EMISSION_BOUND,
        }
    }
}

/// The summary line, contract section 2.2, with the counts the emitter
/// takes over its own points.
#[derive(Debug, Deserialize)]
struct SummaryLine {
    positions: usize,
    with_entropy: usize,
    with_surprisal: usize,
    generations: Vec<GenerationEntry>,
}

/// One summary entry, per generation and in landing order. **Every member
/// but the output count is absent on its own terms** and crosses omitted,
/// never null, per contract section 7; an added member the emitter grows is
/// ignored here, optional at the read.
#[derive(Debug, Clone, Deserialize)]
pub struct GenerationEntry {
    pub turn: Option<String>,
    pub perplexity: Option<f64>,
    pub resident: Option<u64>,
    pub output_count: u64,
    /// The record identity, the sentinel crossing as the empty string it is.
    pub weights_hash: Option<String>,
    /// Absent only from an emitter older than 2026-09-09 (contract 2.2).
    pub run: Option<String>,
    pub session: Option<String>,
    pub digest: Option<String>,
    pub prefix_length: Option<u64>,
    pub effective_sampling: Option<serde_json::Value>,
    pub field_depth: Option<u64>,
    pub lineage: Option<serde_json::Value>,
    pub device_model: Option<String>,
    pub code_identity: Option<serde_json::Value>,
    pub verdict: Option<serde_json::Value>,
}

/// One point, contract section 2.1. The position is not on the wire.
#[derive(Debug, Clone, Deserialize)]
pub struct WirePoint {
    pub turn: Option<String>,
    pub ordinal: u64,
    pub token: u64,
    pub entropy: Option<f64>,
    pub surprisal: Option<f64>,
}

/// An emission read whole: its generations in landing order, each with the
/// points that belong to it.
#[derive(Debug, Clone)]
pub struct Emission {
    pub generations: Vec<(GenerationEntry, Vec<WirePoint>)>,
}

/// Why an emission as a whole cannot be read. Nothing of it lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable(pub String);

/// Read a line no longer than `LINE_BOUND`, without its newline, adding
/// what it read to `total` and refusing past `limit`. `None` at the end of
/// the input.
fn bounded_line(
    input: &mut impl BufRead,
    n: usize,
    total: &mut u64,
    limit: u64,
) -> Result<Option<String>, Unreadable> {
    let mut bytes = Vec::new();
    let read = std::io::Read::take(&mut *input, LINE_BOUND as u64 + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| Unreadable(format!("line {n} could not be read: {e}")))?;
    if read == 0 {
        return Ok(None);
    }
    *total += read as u64;
    if *total > limit {
        return Err(Unreadable(format!(
            "the emission runs past {limit} bytes at line {n}, the reader's bound"
        )));
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    } else if bytes.len() > LINE_BOUND {
        return Err(Unreadable(format!("line {n} runs past {LINE_BOUND} bytes")));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| Unreadable(format!("line {n} is not UTF-8")))
}

impl Emission {
    /// **Read one emission and check its two streams against each other.**
    /// The summary's counts are the emitter's own over its points, and its
    /// output counts partition the points in landing order, each
    /// generation's ordinals running from zero under its turn; anything else
    /// is an emission this reader cannot address and refuses whole.
    pub fn read(input: impl BufRead) -> Result<Self, Unreadable> {
        Self::read_within(input, Bounds::default())
    }

    /// As `read`, under `bounds`. **The emission is held whole within them**:
    /// a summary announcing more positions than the bound is refused before a
    /// point is read, points past the announced count are refused at the
    /// first one over, and the bytes read are bounded however the lines
    /// fall.
    pub fn read_within(mut input: impl BufRead, bounds: Bounds) -> Result<Self, Unreadable> {
        let mut total = 0u64;
        let Some(first) = bounded_line(&mut input, 1, &mut total, bounds.bytes)? else {
            return Err(Unreadable("the emission is empty: no summary line".into()));
        };
        let summary: SummaryLine = serde_json::from_str(&first)
            .map_err(|e| Unreadable(format!("the summary line is not a summary: {e}")))?;
        if summary.generations.iter().any(|g| g.run.is_none()) {
            return Err(Unreadable(
                "the summary names no run identity: an emitter older than 2026-09-09, whose emission keys no row (weaver-analysis-web-contract section 2.2)".into(),
            ));
        }
        if summary.positions > bounds.positions {
            return Err(Unreadable(format!(
                "the summary announces {} positions, past the reader's bound of {}",
                summary.positions, bounds.positions
            )));
        }
        // Grown as points arrive rather than reserved from the announced
        // count, which a summary could overstate.
        let mut points = Vec::new();
        let mut n = 1;
        loop {
            n += 1;
            let Some(line) = bounded_line(&mut input, n, &mut total, bounds.bytes)? else {
                break;
            };
            if line.is_empty() {
                continue;
            }
            if points.len() == summary.positions {
                return Err(Unreadable(format!(
                    "line {n} is a point past the {} the summary announced",
                    summary.positions
                )));
            }
            let point: WirePoint = serde_json::from_str(&line)
                .map_err(|e| Unreadable(format!("line {n} is not a point: {e}")))?;
            points.push(point);
        }
        if points.len() != summary.positions {
            return Err(Unreadable(format!(
                "the summary counts {} positions and the emission carries {}",
                summary.positions,
                points.len()
            )));
        }
        let with_entropy = points.iter().filter(|p| p.entropy.is_some()).count();
        let with_surprisal = points.iter().filter(|p| p.surprisal.is_some()).count();
        if (with_entropy, with_surprisal) != (summary.with_entropy, summary.with_surprisal) {
            return Err(Unreadable(format!(
                "the summary counts {} entropies and {} surprisals and the points carry {with_entropy} and {with_surprisal}",
                summary.with_entropy, summary.with_surprisal
            )));
        }
        let mut counted: u64 = 0;
        for (index, g) in summary.generations.iter().enumerate() {
            counted = counted.checked_add(g.output_count).ok_or_else(|| {
                Unreadable(format!(
                    "the generations' output counts overflow at generation {index}"
                ))
            })?;
        }
        if counted != points.len() as u64 {
            return Err(Unreadable(format!(
                "the generations' output counts sum to {counted} and the emission carries {} points",
                points.len()
            )));
        }
        let mut rest = points.into_iter();
        let mut generations = Vec::with_capacity(summary.generations.len());
        for (index, entry) in summary.generations.into_iter().enumerate() {
            let own: Vec<WirePoint> = rest.by_ref().take(entry.output_count as usize).collect();
            for (j, point) in own.iter().enumerate() {
                if point.ordinal != j as u64 || point.turn != entry.turn {
                    return Err(Unreadable(format!(
                        "generation {index}'s point {j} reads ordinal {} under turn {:?}, not ordinal {j} under turn {:?}",
                        point.ordinal, point.turn, entry.turn
                    )));
                }
            }
            generations.push((entry, own));
        }
        Ok(Self { generations })
    }

    /// **Each run the emission carries, planned**, in the order its first
    /// generation lands. A run is planned or refused on its own; one run's
    /// refusal never stops another's.
    pub fn plan(&self) -> Vec<(String, Result<RunPlan, String>)> {
        let mut order: Vec<String> = Vec::new();
        let mut by_run: HashMap<String, Vec<&(GenerationEntry, Vec<WirePoint>)>> = HashMap::new();
        for generation in &self.generations {
            let run = generation
                .0
                .run
                .clone()
                .expect("read refuses a runless entry");
            if !by_run.contains_key(&run) {
                order.push(run.clone());
            }
            by_run.entry(run).or_default().push(generation);
        }
        order
            .into_iter()
            .map(|run| {
                let planned = RunPlan::of(&run, &by_run[&run]);
                (run, planned)
            })
            .collect()
    }
}

/// A generation whose points cannot be addressed, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Skipped {
    pub seq: i32,
    pub turn: Option<String>,
    pub why: &'static str,
}

/// **One run as it lands**: its row's members, its generations, and its
/// points with their derived positions, each tagged with its generation's
/// order, which the walk reads.
#[derive(Debug, Clone)]
pub struct RunPlan {
    pub members: RunMembers,
    pub generations: Vec<GenerationRow>,
    pub points: Vec<(i32, PositionRow)>,
    /// Each generation's points as a span of `points`, by order, so a
    /// generation's fill reads its own and never scans the run's.
    pub spans: Vec<std::ops::Range<usize>>,
    pub skipped: Vec<Skipped>,
    /// A verdict crossed, which has no column until its kind lands (Spec 2.2).
    pub verdict_crossed: bool,
    /// **The tape repeated a coordinate**: a key two generations named with
    /// one payload, folded to one stored row. The stored rows keep one per
    /// key, and the tape is still one that revisited a position, which the
    /// walk reads as a tape with no coordinate.
    pub repeated: bool,
}

/// A member every entry of one run must agree on, with its name.
fn agreed<T: PartialEq + Clone>(
    entries: &[&(GenerationEntry, Vec<WirePoint>)],
    name: &str,
    member: impl Fn(&GenerationEntry) -> Option<T>,
) -> Result<Option<T>, String> {
    let first = member(&entries[0].0);
    for (seq, entry) in entries.iter().enumerate().skip(1) {
        if member(&entry.0) != first {
            return Err(format!(
                "the run's generations disagree on its {name}: generation {seq} differs from generation 0 (weaver-analysis-web-contract section 2.2)"
            ));
        }
    }
    Ok(first)
}

/// The effective sampling's declared members: the per-generation derived
/// seed removed, since every run of more than one generation disagrees on
/// it by construction (contract 2.2).
fn declared(sampling: &serde_json::Value) -> serde_json::Value {
    let mut declared = sampling.clone();
    if let Some(object) = declared.as_object_mut() {
        object.remove("generation_seed");
    }
    declared
}

/// An unsigned integer as the record spells it, as text, or `None`.
fn unsigned_text(value: Option<&serde_json::Value>) -> Result<Option<String>, ()> {
    match value {
        None => Ok(None),
        Some(v) => v.as_u64().map(|n| Some(n.to_string())).ok_or(()),
    }
}

impl RunPlan {
    fn of(run: &str, entries: &[&(GenerationEntry, Vec<WirePoint>)]) -> Result<Self, String> {
        // **Every rule the schema holds a row to is met while planning**,
        // by each row's `validate` beside its definition in `rows.rs`, and by
        // the conversions below, every integer read unsigned and converted
        // checked; a key named twice is refused below unless both payloads
        // agree, and a turnless generation lands no point.
        // **Agreement, member by member, before anything is formed**: a run
        // whose generations disagree is a defect the reader names, never a
        // run with two of anything (contract 2.2). Presence counts: an entry
        // carrying a member another omits is a disagreement.
        let weights = agreed(entries, "weights hash", |g| g.weights_hash.clone())?;
        let session = agreed(entries, "session", |g| g.session.clone())?;
        let digest = agreed(entries, "digest", |g| g.digest.clone())?;
        let prefix = agreed(entries, "seated prefix's length", |g| g.prefix_length)?;
        let depth = agreed(entries, "field election's depth", |g| g.field_depth)?;
        let lineage = agreed(entries, "lineage", |g| g.lineage.clone())?;
        let device = agreed(entries, "device model", |g| g.device_model.clone())?;
        let engine = agreed(entries, "code identity", |g| g.code_identity.clone())?;
        let sampling = agreed(entries, "declared sampling", |g| {
            g.effective_sampling.as_ref().map(declared)
        })?;
        let verdicts = entries.iter().filter(|e| e.0.verdict.is_some()).count();
        if verdicts > 1 {
            return Err(format!(
                "the run carries {verdicts} verdicts, and a verdict crosses once, on the generation its close names (weaver-analysis-web-contract section 2.2)"
            ));
        }
        // **An absent effective sampling writes no run row** (contract 2.2):
        // the sampler is a condition the row is not a row without.
        let Some(sampler) = sampling else {
            return Err(
                "the run names no effective sampling, a condition its row is not a row without (weaver-analysis-web-contract section 2.2)".into(),
            );
        };
        // **An absent weights hash is not the sentinel**: the sentinel
        // crosses as the empty string and lands, and a run whose every
        // generation omits the member has no record identity to key on.
        let Some(record_identity) = weights else {
            return Err(
                "the run's generations carry no weights hash, so it has no record identity; the sentinel would cross as the empty string".into(),
            );
        };
        let seed = unsigned_text(sampler.get("seed"))
            .map_err(|()| "the effective sampling's seed is not an unsigned integer".to_string())?;
        let parent_reference = match &lineage {
            None => None,
            // **The parent is `built_from`'s run and nothing else**; a
            // lineage without `built_from` is a continuation of its run and
            // names no parent, and `through` is a turn, never a position
            // (Spec 3.1, after toddwbucy/WeaverAgent#58).
            Some(lineage) => match lineage.get("built_from") {
                None | Some(serde_json::Value::Null) => None,
                Some(built) => match built.get("run").and_then(|r| r.as_str()) {
                    Some(parent) => Some(parent.to_owned()),
                    None => {
                        return Err("the lineage's built_from names no run".into());
                    }
                },
            },
        };
        let to_i32 = |n: u64, what: &str| {
            i32::try_from(n)
                .map_err(|_| format!("the run's {what} ({n}) is past the column's range"))
        };
        let members = RunMembers {
            record_identity,
            seed,
            sampler,
            device,
            engine,
            field_depth: depth.map(|d| to_i32(d, "field depth")).transpose()?,
            record_session: session,
            record_digest: digest,
            prefix_length: prefix
                .map(|p| to_i32(p, "seated prefix's length"))
                .transpose()?,
            parent_reference,
            boundary_set: serde_json::json!([]),
        };
        members.validate(run)?;

        let mut generations = Vec::with_capacity(entries.len());
        let mut points: Vec<(i32, PositionRow)> = Vec::new();
        let mut skipped = Vec::new();
        let mut keys: BTreeMap<(String, i32), PositionRow> = BTreeMap::new();
        let mut repeated = false;
        for (seq, (entry, own)) in entries.iter().map(|e| (&e.0, &e.1)).enumerate() {
            let seq = seq as i32;
            let generation_seed = unsigned_text(
                entry
                    .effective_sampling
                    .as_ref()
                    .and_then(|s| s.get("generation_seed")),
            )
            .map_err(|()| {
                format!("generation {seq}'s generation seed is not an unsigned integer")
            })?;
            let generation = GenerationRow {
                seq,
                turn: entry.turn.clone(),
                perplexity: entry.perplexity,
                resident: entry
                    .resident
                    .map(|r| to_i32(r, "resident count"))
                    .transpose()?,
                output_count: to_i32(entry.output_count, "output count")?,
                generation_seed,
            };
            generation.validate()?;
            generations.push(generation);
            // **A generation with no drawn tokens owes no points**: it lands
            // its summary and is neither addressed nor skipped, so it leaves
            // the run whole whatever its resident count or turn key, a run
            // being short only where a point it owed did not land.
            if entry.output_count == 0 {
                continue;
            }
            // **A generation whose points cannot be addressed lands its
            // summary and not its points** (Spec 3.1): no resident count, no
            // position; no turn key, no key. Nothing invents either.
            let (Some(resident), Some(turn)) = (entry.resident, entry.turn.clone()) else {
                skipped.push(Skipped {
                    seq,
                    turn: entry.turn.clone(),
                    why: if entry.resident.is_none() {
                        "no resident count"
                    } else {
                        "no turn key"
                    },
                });
                continue;
            };
            // **The position, derived here and nowhere later**: `(R - O - 1)
            // + j`, the one being the turn terminator the SPU makes resident
            // before the answer returns (Spec 3.1).
            let floor = resident
                .checked_sub(entry.output_count + 1)
                .ok_or_else(|| {
                    format!(
                        "generation {seq}'s resident count {resident} is below its {} drawn tokens and the terminator",
                        entry.output_count
                    )
                })?;
            for point in own {
                let position = to_i32(floor + point.ordinal, "position")?;
                let token_id = i64::try_from(point.token).map_err(|_| {
                    format!(
                        "generation {seq}'s token {} is past the column's range",
                        point.token
                    )
                })?;
                let row = PositionRow {
                    turn: turn.clone(),
                    position,
                    token_id,
                    entropy: point.entropy,
                    surprisal: point.surprisal,
                };
                // **One key, one truth**: a key the run names twice lands
                // once where the two agree and refuses the run where they
                // do not.
                match keys.get(&(turn.clone(), position)) {
                    Some(held) if *held == row => {
                        repeated = true;
                        continue;
                    }
                    Some(_) => {
                        return Err(format!(
                            "the run names turn {turn} position {position} twice with two payloads"
                        ));
                    }
                    None => {
                        keys.insert((turn.clone(), position), row.clone());
                    }
                }
                points.push((seq, row));
            }
        }
        let mut spans = vec![0..0; generations.len()];
        let mut at = 0;
        while at < points.len() {
            let (seq, start) = (points[at].0, at);
            while at < points.len() && points[at].0 == seq {
                at += 1;
            }
            spans[seq as usize] = start..at;
        }
        Ok(Self {
            members,
            generations,
            points,
            spans,
            skipped,
            verdict_crossed: verdicts == 1,
            repeated,
        })
    }

    /// **The status the run closes at**: `short` where some generation's
    /// points could not be addressed, naming each, and `whole` otherwise.
    pub fn closing(&self) -> (&'static str, Option<String>) {
        if self.skipped.is_empty() {
            return ("whole", None);
        }
        let named: Vec<String> = self
            .skipped
            .iter()
            .map(|s| match &s.turn {
                Some(turn) => format!("generation {} (turn {turn}): {}", s.seq, s.why),
                None => format!("generation {}: {}", s.seq, s.why),
            })
            .collect();
        (
            "short",
            Some(format!("points not landed: {}", named.join("; "))),
        )
    }

    /// The members the record did not carry, by the names the answer uses.
    pub fn absent(&self) -> Vec<String> {
        let m = &self.members;
        let mut absent: Vec<String> = Vec::new();
        for (name, missing) in [
            ("seed", m.seed.is_none()),
            ("device", m.device.is_none()),
            ("engine", m.engine.is_none()),
            ("field_depth", m.field_depth.is_none()),
            ("record_session", m.record_session.is_none()),
            ("record_digest", m.record_digest.is_none()),
            ("prefix_length", m.prefix_length.is_none()),
        ] {
            if missing {
                absent.push(name.into());
            }
        }
        // Never on this seam (contract sections 2.1 and 6), and the
        // signature is shingles over a text that does not cross.
        absent.extend(["token_text", "alternatives", "realized", "signature"].map(String::from));
        let of = |name: &str, missing: usize, total: usize, unit: &str| {
            (missing > 0).then(|| format!("{name} (on {missing} of {total} {unit})"))
        };
        let (generations, points) = (self.generations.len(), self.points.len());
        absent.extend(
            [
                of(
                    "perplexity",
                    self.generations
                        .iter()
                        .filter(|g| g.perplexity.is_none())
                        .count(),
                    generations,
                    "generations",
                ),
                of(
                    "entropy",
                    self.points
                        .iter()
                        .filter(|(_, p)| p.entropy.is_none())
                        .count(),
                    points,
                    "positions",
                ),
                of(
                    "surprisal",
                    self.points
                        .iter()
                        .filter(|(_, p)| p.surprisal.is_none())
                        .count(),
                    points,
                    "positions",
                ),
            ]
            .into_iter()
            .flatten(),
        );
        absent
    }
}

#[cfg(test)]
mod tests {
    //! The reader's bounds, over streams built here and no store.

    use super::*;
    use std::io::{BufReader, Read};

    /// A summary announcing `positions` with one generation of that many
    /// points, as text.
    fn summary(positions: usize, generations: &[u64]) -> String {
        let generations: Vec<serde_json::Value> = generations
            .iter()
            .map(|count| serde_json::json!({"turn": "t-1", "output_count": count, "run": "r"}))
            .collect();
        serde_json::json!({
            "positions": positions, "with_entropy": 0, "with_surprisal": 0,
            "generations": generations,
        })
        .to_string()
            + "\n"
    }

    /// A stream of points that never ends, counting the bytes taken from it.
    struct Endless {
        line: Vec<u8>,
        at: usize,
        taken: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut n = 0;
            while n < buf.len() {
                buf[n] = self.line[self.at];
                self.at = (self.at + 1) % self.line.len();
                n += 1;
            }
            self.taken.set(self.taken.get() + n);
            Ok(n)
        }
    }

    /// **Points past the announced count are refused at the first one
    /// over**, so an endless stream never grows the reader: two announced,
    /// and the reader stops within a buffer or two of the third.
    #[test]
    fn points_past_the_announced_count_are_refused_at_the_first() {
        let taken = std::rc::Rc::new(std::cell::Cell::new(0));
        let stream = Endless {
            line: b"{\"turn\":\"t-1\",\"ordinal\":0,\"token\":1}\n".to_vec(),
            at: 0,
            taken: taken.clone(),
        };
        let head = summary(2, &[2]);
        let input = BufReader::new(head.as_bytes().chain(stream));
        let refused = Emission::read(input).unwrap_err();
        assert!(
            refused.0.contains("past the 2 the summary announced"),
            "{}",
            refused.0
        );
        assert!(
            taken.get() < 64 * 1024,
            "read {} bytes of an endless stream",
            taken.get()
        );
    }

    /// **A summary announcing more positions than the bound is refused
    /// before a point is read.**
    #[test]
    fn a_summary_past_the_positions_bound_is_refused() {
        let text = summary(POSITIONS_BOUND + 1, &[POSITIONS_BOUND as u64 + 1]);
        let refused = Emission::read(text.as_bytes()).unwrap_err();
        assert!(
            refused.0.contains("past the reader's bound"),
            "{}",
            refused.0
        );
    }

    /// **The bytes read are bounded however the lines fall**: blank lines
    /// carry no point and still count.
    #[test]
    fn the_bytes_read_are_bounded() {
        let text = summary(0, &[]) + &"\n".repeat(4096);
        let bounds = Bounds {
            positions: POSITIONS_BOUND,
            bytes: 1024,
        };
        let refused = Emission::read_within(text.as_bytes(), bounds).unwrap_err();
        assert!(refused.0.contains("runs past 1024 bytes"), "{}", refused.0);
    }

    /// **Output counts that overflow are refused by name**, never a panic
    /// in a debug build or a wrap in a release one.
    #[test]
    fn output_counts_that_overflow_are_refused() {
        let text = summary(0, &[u64::MAX, 1]);
        let refused = Emission::read(text.as_bytes()).unwrap_err();
        assert!(
            refused.0.contains("overflow at generation 1"),
            "{}",
            refused.0
        );
    }
}
