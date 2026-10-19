//! The script log: `console.*` output and bevy-ts errors and warnings, deduped, kept in a ring
//! for the protocol's `bevy-ts/log+watch`, and mirrored to Bevy's log.

use crate::backend::{ScriptError, SourceLocation};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: u64,
    pub level: LogLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    pub generation: u64,
    /// How many identical messages this entry stands for (deduped errors).
    pub count: u32,
}

const RING: usize = 1024;
const DEDUP_WINDOW: Duration = Duration::from_secs(1);

struct Repeat {
    last_logged: Instant,
    suppressed: u32,
}

/// The ring of recent log entries.
#[derive(Resource, Default)]
pub struct ScriptLog {
    entries: VecDeque<LogEntry>,
    next_seq: u64,
    repeats: HashMap<(LogLevel, String, Option<SourceLocation>), Repeat>,
}

impl ScriptLog {
    /// Logs a message unconditionally.
    pub fn push(&mut self, level: LogLevel, message: impl Into<String>, location: Option<SourceLocation>, generation: u64) {
        self.push_counted(level, message.into(), location, generation, 1);
    }

    fn push_counted(&mut self, level: LogLevel, message: String, location: Option<SourceLocation>, generation: u64, count: u32) {
        let at = location.as_ref().map(|l| format!(" ({l})")).unwrap_or_default();
        let times = if count > 1 { format!(" [x{count}]") } else { String::new() };
        match level {
            LogLevel::Debug => debug!(target: "bevy_ts", "{message}{at}{times}"),
            LogLevel::Info => info!(target: "bevy_ts", "{message}{at}{times}"),
            LogLevel::Warn => warn!(target: "bevy_ts", "{message}{at}{times}"),
            LogLevel::Error => error!(target: "bevy_ts", "{message}{at}{times}"),
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        if self.entries.len() == RING {
            self.entries.pop_front();
        }
        self.entries.push_back(LogEntry { seq, level, message, location, generation, count });
    }

    /// Logs a message at most once per second per (level, message, location); the line that
    /// does get through carries how many it stands for.
    pub fn push_deduped(&mut self, level: LogLevel, message: impl Into<String>, location: Option<SourceLocation>, generation: u64) {
        let message = message.into();
        let key = (level, message.clone(), location.clone());
        let now = Instant::now();
        match self.repeats.get_mut(&key) {
            Some(r) if now.duration_since(r.last_logged) < DEDUP_WINDOW => {
                r.suppressed += 1;
            }
            Some(r) => {
                let count = r.suppressed + 1;
                r.suppressed = 0;
                r.last_logged = now;
                self.push_counted(level, message, location, generation, count);
            }
            None => {
                self.repeats.insert(key, Repeat { last_logged: now, suppressed: 0 });
                self.push_counted(level, message, location, generation, 1);
            }
        }
    }

    /// Logs a script error, deduped, with its TS stack.
    pub fn error(&mut self, context: &str, e: &ScriptError, generation: u64) {
        let message = match &e.stack {
            Some(stack) if !stack.is_empty() => format!("{context}: {}\n{stack}", e.message),
            _ => format!("{context}: {}", e.message),
        };
        self.push_deduped(LogLevel::Error, message, e.location.clone(), generation);
    }

    /// Entries with `seq >= from`.
    pub fn since(&self, from: u64) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter().filter(move |e| e.seq >= from)
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_collapse_within_a_second() {
        let mut log = ScriptLog::default();
        for _ in 0..100 {
            log.push_deduped(LogLevel::Error, "boom", None, 1);
        }
        assert_eq!(log.since(0).count(), 1);
        log.repeats.values_mut().for_each(|r| r.last_logged -= Duration::from_secs(2));
        log.push_deduped(LogLevel::Error, "boom", None, 1);
        let last = log.since(0).last().unwrap();
        assert_eq!(last.count, 100);
    }
}
