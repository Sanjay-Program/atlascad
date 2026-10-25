//! Feature dependency graph and incremental evaluation cache.
//!
//! Every feature reads some objects (sketches, bodies) and writes others. Walking the
//! timeline with a "last writer" table turns that into a DAG: `dependency_graph` reports it
//! statically for the UI and the CLI, and `evaluate_with` uses it at run time to give each
//! feature a content key = hash(definition, parameter values it mentions, keys of the
//! features that produced its inputs). A key seen before means the same inputs and the same
//! operation, so the cached outputs (solids with their persistent names, sketch geometry)
//! are restored instead of calling the kernel. Editing one feature therefore recomputes only
//! its downstream subgraph; undo returns to keys the cache still holds and costs nothing.

use crate::doc::{BoolOp, Document, Feature, PlaneRef};
use crate::eval::SketchInfo;
use crate::expr::Scope;
use crate::sketch::SketchGeometry;
use cadrum::{DVec3, Solid};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Object ids (sketches, bodies) a feature reads.
pub fn inputs(f: &Feature) -> Vec<String> {
    let opt = |t: &Option<String>| t.iter().cloned().collect::<Vec<_>>();
    match f {
        Feature::Sketch { plane: PlaneRef::Face { face }, .. } => vec![face.body.clone()],
        Feature::Sketch { .. } => vec![],
        Feature::Extrude { sketch, target, .. } | Feature::Revolve { sketch, target, .. } => [vec![sketch.clone()], opt(target)].concat(),
        Feature::Sweep { sketch, path, target, .. } => [vec![sketch.clone(), path.clone()], opt(target)].concat(),
        Feature::Loft { sketches, target, .. } => [sketches.clone(), opt(target)].concat(),
        Feature::Box { target, .. } | Feature::Cylinder { target, .. } | Feature::Sphere { target, .. } | Feature::Torus { target, .. } | Feature::Import { target, .. } => opt(target),
        Feature::Fillet { body, .. }
        | Feature::Chamfer { body, .. }
        | Feature::Shell { body, .. }
        | Feature::Move { body, .. }
        | Feature::Scale { body, .. }
        | Feature::Appearance { body, .. }
        | Feature::Hole { body, .. }
        | Feature::OffsetFace { body, .. }
        | Feature::Delete { body, .. }
        | Feature::Split { body, .. }
        | Feature::Mirror { body, .. } => vec![body.clone()],
        Feature::Combine { target, tool, .. } => vec![target.clone(), tool.clone()],
        Feature::Pattern { body, join, .. } | Feature::CircularPattern { body, join, .. } => [vec![body.clone()], opt(join)].concat(),
    }
}

/// Object ids a feature writes (created or modified) and removes, from the definition alone.
/// Extra bodies that only show up at run time (`id_1` pieces) are not listed.
pub fn outputs(f: &Feature) -> (Vec<String>, Vec<String>) {
    let maker = |id: &String, op: &BoolOp, target: &Option<String>| match (op, target) {
        (BoolOp::NewBody, _) | (_, None) => (vec![id.clone()], vec![]),
        (_, Some(t)) => (vec![t.clone()], vec![]),
    };
    match f {
        Feature::Sketch { id, .. } => (vec![id.clone()], vec![]),
        Feature::Extrude { id, op, target, .. }
        | Feature::Revolve { id, op, target, .. }
        | Feature::Sweep { id, op, target, .. }
        | Feature::Loft { id, op, target, .. }
        | Feature::Box { id, op, target, .. }
        | Feature::Cylinder { id, op, target, .. }
        | Feature::Sphere { id, op, target, .. }
        | Feature::Torus { id, op, target, .. }
        | Feature::Import { id, op, target, .. } => maker(id, op, target),
        Feature::Fillet { body, .. }
        | Feature::Chamfer { body, .. }
        | Feature::Shell { body, .. }
        | Feature::Move { body, .. }
        | Feature::Scale { body, .. }
        | Feature::Appearance { body, .. }
        | Feature::Hole { body, .. }
        | Feature::OffsetFace { body, .. } => (vec![body.clone()], vec![]),
        Feature::Delete { body, .. } => (vec![], vec![body.clone()]),
        Feature::Split { id, body, .. } => (vec![body.clone(), id.clone()], vec![]),
        Feature::Combine { target, tool, keep_tool, .. } => (vec![target.clone()], if *keep_tool { vec![] } else { vec![tool.clone()] }),
        Feature::Mirror { id, body, join, .. } => (vec![if *join { body.clone() } else { id.clone() }], vec![]),
        Feature::Pattern { body, join, .. } | Feature::CircularPattern { body, join, .. } => match join {
            Some(t) => (vec![t.clone()], vec![body.clone()]),
            None => (vec![body.clone()], vec![]),
        },
    }
}

/// Feature ids each feature depends on (same order as the timeline).
pub fn dependency_graph(doc: &Document) -> Vec<Vec<String>> {
    let mut producer: HashMap<String, String> = HashMap::new();
    let mut out = Vec::with_capacity(doc.timeline.len());
    for f in &doc.timeline {
        let mut deps: Vec<String> = inputs(f).iter().filter_map(|id| producer.get(id).cloned()).collect();
        deps.dedup();
        out.push(deps);
        let (written, removed) = outputs(f);
        for id in removed {
            producer.remove(&id);
        }
        for id in written {
            producer.insert(id, f.id().to_string());
        }
    }
    out
}

/// Graphviz DOT text of the dependency graph.
pub fn dot(doc: &Document) -> String {
    let deps = dependency_graph(doc);
    let mut s = String::from("digraph cadrust {\n  rankdir=LR;\n  node [shape=box, fontname=\"Helvetica\"];\n");
    for (f, d) in doc.timeline.iter().zip(&deps) {
        s.push_str(&format!("  \"{}\" [label=\"{}\\n{}\"];\n", f.id(), f.kind(), f.id()));
        for up in d {
            s.push_str(&format!("  \"{up}\" -> \"{}\";\n", f.id()));
        }
    }
    s.push_str("}\n");
    s
}

/// True when `name` appears as a whole word in `text` (an expression mentions a parameter).
fn mentions(text: &str, name: &str) -> bool {
    let b = text.as_bytes();
    let mut from = 0;
    while let Some(pos) = text[from..].find(name) {
        let (s, e) = (from + pos, from + pos + name.len());
        let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
        if (s == 0 || !word(b[s - 1])) && (e == b.len() || !word(b[e])) {
            return true;
        }
        from = e;
    }
    false
}

/// Content key of a feature in its current context.
pub fn feature_key(f: &Feature, scope: &Scope, suppressed: bool, param_names: &[String], last: &HashMap<String, u64>) -> u64 {
    let json = serde_json::to_string(f).unwrap_or_default();
    let mut h = DefaultHasher::new();
    json.hash(&mut h);
    suppressed.hash(&mut h);
    for name in param_names {
        if mentions(&json, name) {
            name.hash(&mut h);
            scope.get(name).unwrap_or(f64::NAN).to_bits().hash(&mut h);
        }
    }
    for id in inputs(f) {
        id.hash(&mut h);
        last.get(&id).copied().unwrap_or(0).hash(&mut h);
    }
    h.finish()
}

/// What a feature produced, kept so a later evaluation can replay it without the kernel.
pub struct Entry {
    gen: u64,
    /// Bodies created or replaced: id, solid, (face center, persistent name) pairs.
    pub outputs: Vec<(String, Solid, Vec<(DVec3, String)>)>,
    pub removed: Vec<String>,
    pub sketch: Option<(SketchGeometry, SketchInfo)>,
    pub warning: Option<String>,
    pub names: Option<Vec<String>>,
}

/// Memo of feature outputs across evaluations of one document (one per server, worker or
/// session). Entries unused for `keep` evaluations are dropped, so undo depth stays cheap.
pub struct EvalCache {
    entries: HashMap<u64, Entry>,
    generation: u64,
    pub keep: u64,
    pub max_entries: usize,
    pub hits: usize,
    pub misses: usize,
}

impl Default for EvalCache {
    fn default() -> Self {
        EvalCache { entries: HashMap::new(), generation: 0, keep: 60, max_entries: 600, hits: 0, misses: 0 }
    }
}

impl EvalCache {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub(crate) fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }
    pub(crate) fn finish(&mut self) {
        let min = self.generation.saturating_sub(self.keep);
        self.entries.retain(|_, e| e.gen >= min);
        if self.entries.len() > self.max_entries {
            let mut gens: Vec<u64> = self.entries.values().map(|e| e.gen).collect();
            gens.sort_unstable();
            let cut = gens[self.entries.len() - self.max_entries];
            self.entries.retain(|_, e| e.gen > cut);
        }
    }
    pub(crate) fn get(&mut self, key: u64) -> Option<&Entry> {
        let gen = self.generation;
        match self.entries.get_mut(&key) {
            Some(e) => {
                e.gen = gen;
                self.hits += 1;
                Some(&*e)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }
    pub(crate) fn put(&mut self, key: u64, mut entry: Entry) {
        entry.gen = self.generation;
        self.entries.insert(key, entry);
    }
}

impl Entry {
    pub(crate) fn new(outputs: Vec<(String, Solid, Vec<(DVec3, String)>)>, removed: Vec<String>, sketch: Option<(SketchGeometry, SketchInfo)>, warning: Option<String>, names: Option<Vec<String>>) -> Self {
        Entry { gen: 0, outputs, removed, sketch, warning, names }
    }
}
