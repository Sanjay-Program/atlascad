//! Closing a document keeps only the history that outlives it: the changes stay in the History
//! panel while the document is open (and undo goes with the document anyway), but once closed
//! only the versions, the workspaces' current states and the last healthy marks still at a
//! kept entry are worth the space. A log of 700 changes to a 10 MB board was 213 MB and took 4 s
//! to read when the document opened.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::workspaces::Track;
use super::{Delta, HealthyMark, HistoryLog, LogEntry, RonCache, SNAPSHOT_EVERY, track_state_at, writer};
use crate::document::Document;
use crate::store::Store;

/// A workspace's changes compacted: only the kept entries, and where each of them went.
struct Kept {
    entries: Vec<LogEntry>,
    snapshots: Vec<(usize, Arc<Document>)>,
    healthy: Vec<HealthyMark>,
    /// (old index, new index), old order.
    moved: Vec<(usize, usize)>,
}

impl Kept {
    fn index(&self, old: usize) -> Option<usize> {
        self.moved.iter().find(|(o, _)| *o == old).map(|(_, n)| *n)
    }
}

/// The changes of one workspace with only entries `keep` (its last always among them, and never
/// empty): the first becomes **Start** with a full copy, each other one the change from the one
/// before it, with a full copy every [`SNAPSHOT_EVERY`] entries as usual. `None` if every entry
/// is kept already.
fn compact_track(entries: &[LogEntry], snapshots: &[(usize, Arc<Document>)], healthy: &[HealthyMark], keep: &BTreeSet<usize>) -> Option<Kept> {
    if keep.len() == entries.len() {
        return None;
    }
    let mut kept = Kept { entries: Vec::with_capacity(keep.len()), snapshots: Vec::new(), healthy: Vec::new(), moved: Vec::new() };
    let mut before: Option<Document> = None;
    for &k in keep {
        let state = track_state_at(entries, snapshots, k)?;
        let i = kept.entries.len();
        let mut e = entries[k].clone();
        match &before {
            None => {
                e.element = None;
                e.element_name.clear();
                e.action = "Start".into();
                e.feature = None;
                e.feature_name.clear();
                e.label = "Start".into();
                e.delta = Arc::default();
            }
            Some(b) => e.delta = Arc::new(Delta::between(b, &state)),
        }
        kept.entries.push(e);
        if i.is_multiple_of(SNAPSHOT_EVERY) {
            kept.snapshots.push((i, Arc::new(state.clone())));
        }
        kept.moved.push((k, i));
        before = Some(state);
    }
    kept.healthy = healthy.iter().filter_map(|m| Some(HealthyMark { entry: kept.index(m.entry)?, ..*m })).collect();
    Some(kept)
}

impl HistoryLog {
    /// Drops every change but those versions point at and each workspace's last (see the module
    /// docs): what is left starts at the oldest of them (**Start**), and every state kept is the
    /// same as before. Last healthy marks at a dropped entry go. Returns true if anything was
    /// dropped.
    pub fn compact(&mut self) -> bool {
        let keep = |log: &Self, workspace, len: usize| -> BTreeSet<usize> {
            log.versions_in(workspace).map(|(_, v)| v.entry).filter(|e| *e < len).chain([len - 1]).collect()
        };
        let mut changed = false;
        let current = keep(self, self.current, self.entries.len());
        if let Some(k) = compact_track(&self.entries, &self.snapshots, &self.healthy, &current) {
            let here = self.current;
            for v in self.versions.iter_mut().filter(|v| v.workspace == here) {
                v.entry = k.index(v.entry).unwrap_or(0);
            }
            (self.entries, self.snapshots, self.healthy) = (k.entries, k.snapshots, k.healthy);
            changed = true;
        }
        for t in 0..self.parked.len() {
            let Track { workspace, entries, snapshots, healthy } = &self.parked[t];
            let (workspace, keep) = (*workspace, keep(self, *workspace, entries.len()));
            let Some(k) = compact_track(entries, snapshots, healthy, &keep) else { continue };
            for v in self.versions.iter_mut().filter(|v| v.workspace == workspace) {
                v.entry = k.index(v.entry).unwrap_or(0);
            }
            self.parked[t] = Track { workspace, entries: k.entries, snapshots: k.snapshots, healthy: k.healthy };
            changed = true;
        }
        if changed {
            // (Entries moved: none of the RON written so far is theirs any more.)
            self.ron = RonCache::default();
        }
        changed
    }

    /// The document was closed: writes its log [compacted](Self::compact) next to it, in the
    /// background like [`Self::save_in_background`].
    pub fn save_compacted_in_background(mut self, store: &Store) {
        let path = Self::path(store, self.document);
        writer::queue(path, move || {
            self.compact();
            self.to_ron()
        });
    }
}
