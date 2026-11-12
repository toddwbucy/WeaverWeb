//! **What admin answers a verb in each state the agent row can read**: the
//! admin-operator contract's state table read from the caller's side
//! (`weaver-admin-operator-contract` section 4.1 at [`STATE_TABLE_READ_AT`];
//! Spec 2.13), for the agent page's warnings. **A warning, never a
//! refusal** (the operator's "warn, do not refuse" of 2026-11-11): the page
//! still offers the verb, and the box's own rule decides last.
//!
//! The row reads one of four things, each covering the states of that table
//! whose `show` prints it: `unloaded` (S0, S0d and S8b once nothing of the
//! agent stands), `idle` (S2 and S9), `active` (S3), and a transition
//! standing beside the last state (S0d and S8b while processes stand, S1, S4
//! to S8a, S10, S11, any state while another command holds the agent, and a
//! command killed mid-command), which is the reading whenever one stands
//! (Spec 2.12: a turn's start or close keeps it). **A verb is warned of only
//! where every state its reading covers refuses it**, so a warning is never
//! a guess between an answer and a refusal. The table and its two paragraphs
//! are held beside this module as read (`admin-table-a3543b31.md`, the
//! contract's lines 273 to 318 at the pin, byte for byte), and a test derives
//! every cell from that copy. Where the contract and this module differ, the
//! contract wins.

use chrono::{DateTime, Utc};

use crate::link::register::Agent;

/// **The WeaverAgent commit the contract's state table was read at**. A
/// test holds it to the build's pin,
/// [`crate::declaration::render::CORPUS_COMMIT`], so the pin does not move
/// until the table is reread at the new commit and this module with it.
pub const STATE_TABLE_READ_AT: &str = "a3543b31f264b269f66ab55d8767f25290ec9d61";

/// What the agent row last read of the agent's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    Unloaded,
    Idle,
    Active,
    InTransition,
}

impl Reading {
    /// Every reading, for the tests that walk the table.
    pub const ALL: [Reading; 4] = [
        Reading::Unloaded,
        Reading::Idle,
        Reading::Active,
        Reading::InTransition,
    ];

    /// **The row's last reading and when it was taken**: a transition
    /// standing where one does, the state admin last named otherwise, and
    /// `None` where the row holds no state admin named.
    pub fn of(row: &Agent) -> Option<(Reading, DateTime<Utc>)> {
        if let Some(at) = row.in_transition_at {
            return Some((Reading::InTransition, at));
        }
        let at = row.load_state_at?;
        let reading = match row.load_state.as_deref()? {
            "unloaded" => Reading::Unloaded,
            "idle" => Reading::Idle,
            "active" => Reading::Active,
            _ => return None,
        };
        Some((reading, at))
    }

    /// The reading as the page says it.
    pub fn words(self) -> &'static str {
        match self {
            Reading::Unloaded => "unloaded",
            Reading::Idle => "idle",
            Reading::Active => "active",
            Reading::InTransition => "in transition",
        }
    }
}

/// **The refusal admin gives `verb` in every state `reading` covers**, by
/// the contract's table; `None` where any covered state answers it, or where
/// the table holds no column for the verb (`show`, `validate`, `stop`).
///
/// - `load`: S2, S3 and S9 print `AgentRunning`. In transition, a command
///   holding the agent's lock (S1, S4 to S8a, S10, S11, any other command)
///   makes it `InvocationInFlight`, and with the lock free (S0d and S8b while
///   processes stand, a command killed mid-command) it is `AgentRunning`. S0
///   answers with the load, so `unloaded` warns of nothing.
/// - `save-point`: S0, S0d and S8b print `OutOfOrder`, and S3
///   `ActivityNotAtRest`. In transition it is `InvocationInFlight` with the
///   lock held and `OutOfOrder` with it free. S2 and S9 take it
///   (`SavePointTaken`), so `idle` warns of nothing.
/// - `unload`, `force-unload` and `restore` are answered in some state of
///   every reading (nothing to do, the unload, a join, `RestoreNamed`), so
///   none is warned of.
pub fn refused(verb: &str, reading: Reading) -> Option<&'static str> {
    match (verb, reading) {
        ("load", Reading::Idle | Reading::Active) => Some("AgentRunning"),
        ("load", Reading::InTransition) => {
            Some("InvocationInFlight where a command holds the agent, AgentRunning where none does")
        }
        ("save-point", Reading::Unloaded) => Some("OutOfOrder"),
        ("save-point", Reading::Active) => Some("ActivityNotAtRest"),
        ("save-point", Reading::InTransition) => {
            Some("InvocationInFlight where a command holds the agent, OutOfOrder where none does")
        }
        _ => None,
    }
}

/// **The warning beside an offered verb**, where the row's last reading is
/// one the table refuses it in: the refusal, the reading and its date.
pub fn warning(verb: &str, row: &Agent) -> Option<String> {
    let (reading, at) = Reading::of(row)?;
    let refusal = refused(verb, reading)?;
    let words = reading.words();
    Some(format!(
        "admin refuses this while the agent reads {words}: {refusal}. Read {words} {}.",
        at.format("%Y-%m-%d %H:%M:%S UTC")
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Reading, STATE_TABLE_READ_AT, refused};

    /// The contract's section 4.1 table and the two paragraphs beneath it,
    /// lines 273 to 318 of `weaver-admin-operator-contract.md` at
    /// [`STATE_TABLE_READ_AT`], byte for byte.
    const READ: &str = include_str!("admin-table-a3543b31.md");

    /// The verbs the table gives a column, in the header's order after
    /// `State`, `show` last.
    const COLUMNS: [&str; 6] = [
        "load",
        "unload",
        "force-unload",
        "save-point",
        "restore",
        "show",
    ];

    /// What a cell says of a verb, with the words of the copy that say so.
    #[derive(Clone, Copy)]
    enum Cell {
        /// admin answers it, or acts on it.
        Answers(&'static str),
        /// admin refuses it with this class.
        Refuses(&'static str, &'static str),
        /// The copy says nothing for this state; never warned of.
        Silent(&'static str),
    }
    use Cell::{Answers, Refuses, Silent};

    /// **One state of the table as the row can read it**: where its words
    /// stand (a table row by its leading words, or a paragraph), the
    /// reading its `show` gives with the words that print it, and each
    /// verb's cell, in [`COLUMNS`]' order without `show`.
    struct Case {
        state: &'static str,
        row: Option<&'static str>,
        reading: Reading,
        show: &'static str,
        cells: [Cell; 5],
    }

    const IIF: &str = "InvocationInFlight";

    fn cases() -> Vec<Case> {
        vec![
            Case {
                state: "S0",
                row: Some("S0, down, clean"),
                reading: Reading::Unloaded,
                show: "`Unloaded`",
                cells: [
                    Answers("the load's answer, or its refusal"),
                    Answers("`Unloaded`, nothing to do"),
                    Answers("`Unloaded`, nothing to do"),
                    Refuses("OutOfOrder", "`OutOfOrder`"),
                    Answers("`RestoreNamed`"),
                ],
            },
            Case {
                state: "S0d, nothing standing",
                row: Some("S0d, down, a run ended without closing"),
                reading: Reading::Unloaded,
                show: "`Unloaded`, or",
                cells: [
                    Answers("as S0, the load recording the reset"),
                    Answers(
                        "then publishes the save points the run left as recovered, then `Unloaded`",
                    ),
                    Answers("ends them and publishes the same way, then `Unloaded`"),
                    Refuses("OutOfOrder", "`OutOfOrder`"),
                    Answers("`RestoreNamed`"),
                ],
            },
            Case {
                state: "S0d, processes standing",
                row: Some("S0d, down, a run ended without closing"),
                reading: Reading::InTransition,
                show: "`InTransition` naming the processes of the ended run still standing",
                cells: [
                    Refuses(
                        "AgentRunning",
                        "`AgentRunning` naming them where processes of the ended run are still standing",
                    ),
                    Answers("ends any process of the ended run still standing"),
                    Answers("ends them and publishes the same way"),
                    Refuses("OutOfOrder", "`OutOfOrder`"),
                    Answers("`RestoreNamed`"),
                ],
            },
            Case {
                state: "S1",
                row: Some("S1, loading"),
                reading: Reading::InTransition,
                show: "`InTransition`",
                cells: [
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`, a load being no unload to join"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                ],
            },
            Case {
                state: "S2",
                row: Some("S2, serving at rest"),
                reading: Reading::Idle,
                show: "`Idle`",
                cells: [
                    Refuses("AgentRunning", "`AgentRunning`"),
                    Answers("the graceful unload, below"),
                    Answers("the forced unload, below"),
                    Answers("`SavePointTaken`"),
                    Answers("`RestoreNamed`"),
                ],
            },
            Case {
                state: "S3",
                row: Some("S3, serving, a turn in flight"),
                reading: Reading::Active,
                show: "`Active`",
                cells: [
                    Refuses("AgentRunning", "`AgentRunning`"),
                    Answers("the graceful unload, after the turn"),
                    Answers("the forced unload, the turn cancelled"),
                    Refuses("ActivityNotAtRest", "`ActivityNotAtRest`"),
                    Answers("`RestoreNamed`"),
                ],
            },
            Case {
                state: "S4 to S7",
                row: Some("S4 to S7, a graceful unload in progress"),
                reading: Reading::InTransition,
                show: "`InTransition`",
                cells: [
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Answers("joins, below"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                ],
            },
            Case {
                state: "S8a",
                row: Some("S8a, an unload concluding"),
                reading: Reading::InTransition,
                show: "`InTransition`",
                cells: [
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Answers("waits for the unload to finish, then answers as `show` would"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                ],
            },
            Case {
                state: "S8b, processes standing",
                row: Some(
                    "S8b, the worker gone with no answer (it died), a process still holding the run",
                ),
                reading: Reading::InTransition,
                show: "`InTransition` with the run's processes named while a process holds the run",
                cells: [
                    Refuses(
                        "AgentRunning",
                        "`AgentRunning` until the run ends, then as S0d",
                    ),
                    Answers("ends what still holds the run"),
                    Answers("the same as `unload`"),
                    Refuses("OutOfOrder", "`OutOfOrder`"),
                    Answers("`RestoreNamed`"),
                ],
            },
            Case {
                state: "S8b, nothing standing",
                row: Some(
                    "S8b, the worker gone with no answer (it died), a process still holding the run",
                ),
                reading: Reading::Unloaded,
                show: "`Unloaded` only once none does",
                cells: [
                    Answers("then as S0d"),
                    Answers(
                        "ends what still holds the run, then publishes what the run left as recovered",
                    ),
                    Answers("the same as `unload`"),
                    Refuses("OutOfOrder", "`OutOfOrder`"),
                    Answers("`RestoreNamed`"),
                ],
            },
            Case {
                state: "S9",
                row: Some("S9, an unload stopped at its save point"),
                reading: Reading::Idle,
                show: "`Idle`, the gate lowered",
                cells: [
                    Refuses("AgentRunning", "`AgentRunning`"),
                    Answers("retried: the save point, then down"),
                    Answers("the forced unload"),
                    Answers("`SavePointTaken`"),
                    Answers("`RestoreNamed`"),
                ],
            },
            Case {
                state: "S10",
                row: Some("S10, a force in progress"),
                reading: Reading::InTransition,
                show: "`InTransition`",
                cells: [
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                ],
            },
            Case {
                state: "S11",
                row: Some("S11, a silent worker being ended"),
                reading: Reading::InTransition,
                show: "`InTransition`",
                cells: [
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Answers("beside an `unload` ending it, joins"),
                    Refuses(IIF, "`InvocationInFlight`"),
                    Refuses(IIF, "`InvocationInFlight`"),
                ],
            },
            Case {
                state: "another command holding the agent, in any state",
                row: None,
                reading: Reading::InTransition,
                show: "`InTransition` at once where another command holds the agent",
                cells: [
                    Refuses(
                        IIF,
                        "a line that meets a held lock refuses `InvocationInFlight` whatever the state behind it",
                    ),
                    Refuses(
                        IIF,
                        "a line that meets a held lock refuses `InvocationInFlight` whatever the state behind it",
                    ),
                    Refuses(
                        IIF,
                        "a line that meets a held lock refuses `InvocationInFlight` whatever the state behind it",
                    ),
                    Refuses(
                        IIF,
                        "a line that meets a held lock refuses `InvocationInFlight` whatever the state behind it",
                    ),
                    Refuses(
                        IIF,
                        "a line that meets a held lock refuses `InvocationInFlight` whatever the state behind it",
                    ),
                ],
            },
            Case {
                state: "a command killed mid-command, the lock free",
                row: None,
                reading: Reading::InTransition,
                show: "`InTransition` with the run's processes named where no agent listens and a process still holds the run",
                cells: [
                    Refuses("AgentRunning", "`load` refuses `AgentRunning`"),
                    Answers("or ends a run whose agent no longer listens"),
                    Answers("`force-unload` ends whatever stands"),
                    Refuses("OutOfOrder", "`save-point` refuses `OutOfOrder`"),
                    Silent(
                        "`restore` names the save point the next load restores in every state that answers it",
                    ),
                ],
            },
        ]
    }

    /// The copy with every run of white space one space, so a passage is
    /// found across the copy's line breaks.
    fn flat(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The table row whose state cell is `state`, cut into its cells.
    fn row_of(state: &str) -> Vec<String> {
        let line = READ
            .lines()
            .find(|l| l.starts_with(&format!("| {state} |")))
            .unwrap_or_else(|| panic!("the copy holds no row {state:?}"));
        let cells: Vec<String> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_owned())
            .collect();
        assert_eq!(cells.len(), 7, "row {state:?} has {} cells", cells.len());
        cells
    }

    fn words(cell: Cell) -> &'static str {
        match cell {
            Answers(w) | Refuses(_, w) | Silent(w) => w,
        }
    }

    /// **Every cell of the module stands on the contract's words, and every
    /// warning is derived from them**: each case's words are found in its
    /// row's cell (or in the paragraphs beneath the table), every table row
    /// is read, and for every verb and reading the module warns exactly
    /// where every case of that reading refuses, naming every class those
    /// cases give and no other.
    #[test]
    fn the_state_table_cells_follow_the_contract_as_read() {
        let header = READ.lines().next().expect("the copy is empty");
        assert_eq!(
            header,
            "| State | `load` | `unload` | `force-unload` | `save-point` | `restore` | `show` |",
            "the table's columns moved"
        );
        let paragraphs = flat(
            READ.lines()
                .skip_while(|l| l.starts_with('|'))
                .collect::<Vec<_>>()
                .join("\n")
                .as_str(),
        );
        let cases = cases();
        // Every table row is read by some case.
        for line in READ.lines().skip(2).take_while(|l| l.starts_with('|')) {
            let state = line.trim_start_matches("| ").split(" |").next().unwrap();
            assert!(
                cases.iter().any(|c| c.row == Some(state)),
                "no case reads the table's row {state:?}"
            );
        }
        // Each case's words stand where it says they do.
        for case in &cases {
            let found = |column: usize, w: &str| match case.row {
                Some(row) => flat(&row_of(row)[column + 1]).contains(w),
                None => paragraphs.contains(w),
            };
            assert!(
                found(5, case.show),
                "{}: `show` does not print {:?}",
                case.state,
                case.show
            );
            for (i, cell) in case.cells.iter().enumerate() {
                assert!(
                    found(i, words(*cell)),
                    "{} x {}: the copy does not say {:?}",
                    case.state,
                    COLUMNS[i],
                    words(*cell)
                );
                if let (Refuses(class, _), Some(_)) = (cell, case.row) {
                    assert!(
                        found(i, &format!("`{class}`")),
                        "{} x {}: the cell names no {class}",
                        case.state,
                        COLUMNS[i]
                    );
                }
            }
        }
        // The warnings are the derivation's, cell for cell.
        let classes: BTreeSet<&str> = cases
            .iter()
            .flat_map(|c| c.cells)
            .filter_map(|cell| match cell {
                Refuses(class, _) => Some(class),
                _ => None,
            })
            .collect();
        for (i, verb) in COLUMNS[..5].iter().enumerate() {
            for reading in Reading::ALL {
                let of: Vec<Cell> = cases
                    .iter()
                    .filter(|c| c.reading == reading)
                    .map(|c| c.cells[i])
                    .collect();
                assert!(!of.is_empty(), "no case reads {reading:?}");
                let refusing: Option<BTreeSet<&str>> = of
                    .iter()
                    .map(|cell| match cell {
                        Refuses(class, _) => Some(*class),
                        _ => None,
                    })
                    .collect();
                match (refusing, refused(verb, reading)) {
                    (None, None) => {}
                    (None, Some(w)) => {
                        panic!("{verb} read {reading:?} is warned ({w}), and a case answers it")
                    }
                    (Some(set), None) => panic!(
                        "{verb} read {reading:?} is refused in every case ({set:?}), and not warned"
                    ),
                    (Some(set), Some(w)) => {
                        for class in &classes {
                            assert_eq!(
                                w.contains(class),
                                set.contains(class),
                                "{verb} read {reading:?}: the warning {w:?} and the cases' {set:?} differ on {class}"
                            );
                        }
                    }
                }
            }
        }
        // A verb the table gives no column is never warned of.
        for verb in ["show", "validate", "stop"] {
            for reading in Reading::ALL {
                assert_eq!(refused(verb, reading), None, "{verb} read {reading:?}");
            }
        }
    }

    /// **The pin moves only with a reread**: the table was read at the
    /// commit the build pins.
    #[test]
    fn the_state_table_was_read_at_the_builds_pin() {
        assert_eq!(
            STATE_TABLE_READ_AT,
            crate::declaration::render::CORPUS_COMMIT,
            "the build's pin moved: reread the contract's section 4.1 at the new commit, \
             refresh admin-table-<commit>.md and this module, then move STATE_TABLE_READ_AT"
        );
    }
}
