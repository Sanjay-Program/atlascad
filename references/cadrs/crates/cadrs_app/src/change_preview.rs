//! The feature list's hover over an Add or a Remove (an extrude's, a revolve's): the body it
//! adds (green) or the material it takes out (red), over the parts and through their walls, as
//! its dialog's preview shows it ([`Change`]). Worked out on the kernel thread when the row is
//! hovered ([`cadrs_core::rebuild::request_stage`]); the parts on screen stay as they are.

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use cadrs_core::parts::Part;
use cadrs_core::rebuild::{PendingJob, Stage};
use cadrs_core::{BooleanOp, Feature, FeatureId};

use crate::part_shading::PartShading;
use crate::parts::{Change, ChangeEdgeGizmos, PartCache, PartOverride};
use crate::viewport::{Pick, PlaneHighlight};
use crate::{ActiveDocument, AppState};

pub struct ChangePreviewPlugin;

impl Plugin for ChangePreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoverChange>().add_systems(
            Update,
            (update_hover_change, sync_hover_meshes, draw_hover_edges)
                .chain()
                .after(crate::parts::PartsSet)
                .run_if(in_state(AppState::Document)),
        );
    }
}

/// The change of the feature hovered in the list.
#[derive(Resource, Default)]
pub struct HoverChange {
    /// The hovered feature and the features it is worked out for.
    key: Option<(FeatureId, Vec<Feature>)>,
    pending: Option<PendingJob<Option<Stage>>>,
    /// What is drawn: the change and its bodies.
    pub shown: Option<(Change, Vec<Part>)>,
    /// Bumped whenever `shown` changes.
    generation: u64,
}

impl HoverChange {
    fn show(&mut self, shown: Option<(Change, Vec<Part>)>) {
        if self.shown.is_some() || shown.is_some() {
            self.shown = shown;
            self.generation += 1;
        }
    }
}

/// The hovered feature's change, if it has one: an extrude or a revolve that adds or removes.
fn hovered_change(f: &Feature) -> Option<Change> {
    let op = f.extrude().map(|e| e.op).or(f.revolve().map(|r| r.op))?;
    match op {
        BooleanOp::Add => Some(Change::Add),
        BooleanOp::Remove => Some(Change::Remove),
        _ => None,
    }
}

/// Starts working out the hovered feature's change, and takes it when it is done.
fn update_hover_change(
    highlight: Res<PlaneHighlight>,
    doc: Option<Res<ActiveDocument>>,
    over: Res<PartOverride>,
    cache: Res<PartCache>,
    mut hc: ResMut<HoverChange>,
) {
    // Not while a dialog edits a feature (its own preview shows) or a rebuild runs.
    let target = match (highlight.list, doc.as_deref()) {
        (Some(Pick::Feature(id)), Some(doc)) if over.editing.is_none() && cache.assembly.is_none() && !cache.errors.contains_key(&id) => {
            doc.active_element().and_then(|el| {
                let change = hovered_change(el.feature(id)?)?;
                let features = crate::parts::effective_features(&el.active_features(), &over);
                features.iter().any(|f| f.id == id).then_some((id, change, features))
            })
        }
        _ => None,
    };
    if cache.rebuilding && target.is_some() {
        return;
    }
    let same = match (&hc.key, &target) {
        (Some((a, fa)), Some((b, _, fb))) => a == b && fa == fb,
        (None, None) => true,
        _ => false,
    };
    if !same {
        hc.show(None);
        hc.pending = None;
        hc.key = target.as_ref().map(|(id, _, f)| (*id, f.clone()));
        hc.pending = target.map(|(id, _, f)| cadrs_core::rebuild::request_stage(f, id));
    }
    let Some(done) = hc.pending.as_ref().and_then(|p| p.poll()) else {
        return;
    };
    hc.pending = None;
    let shown = done.flatten().and_then(|st| {
        let id = hc.key.as_ref()?.0;
        // A hovered Add that joins nothing has no stage; a Sheet metal model's isn't a change.
        let change = if st.remove { Change::Remove } else { Change::Add };
        let want = doc.as_deref()?.active_element()?.feature(id).and_then(hovered_change)?;
        (change == want).then(|| (change, crate::parts::stage_bodies(id, &st, u32::MAX - 1000)))
    });
    hc.show(shown);
}

/// The mesh of a body of the hovered feature's change.
#[derive(Component)]
struct HoverChangeMesh;

/// Keeps one mesh per body of the hovered change, drawn by the overlay camera (over the parts).
fn sync_hover_meshes(
    hc: Res<HoverChange>,
    q: Query<Entity, With<HoverChangeMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<PartShading>>,
    mut material: Local<Option<Handle<PartShading>>>,
    mut last: Local<u64>,
    mut commands: Commands,
) {
    if *last == hc.generation {
        return;
    }
    *last = hc.generation;
    for e in &q {
        commands.entity(e).try_despawn();
    }
    let Some((change, parts)) = &hc.shown else {
        return;
    };
    let material = material.get_or_insert_with(|| materials.add(PartShading::new(true, true))).clone();
    for part in parts {
        let bases = vec![change.base(); part.solid.faces.len()];
        commands.spawn((
            Name::new("hover-change"),
            HoverChangeMesh,
            Mesh3d(meshes.add(crate::parts::part_base_mesh(part, true, &bases))),
            MeshMaterial3d(material.clone()),
            Transform::IDENTITY,
            RenderLayers::layer(crate::viewport::OVERLAY_LAYER),
            DespawnOnExit(AppState::Document),
        ));
    }
}

/// The hovered change's edges, over everything with it.
fn draw_hover_edges(hc: Res<HoverChange>, mut g: Gizmos<ChangeEdgeGizmos>) {
    let Some((change, parts)) = &hc.shown else {
        return;
    };
    for part in parts {
        for e in part.solid.edges.iter().filter(|e| !e.smooth) {
            g.linestrip(e.points.iter().map(|p| Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)), change.edge());
        }
    }
}
