//! The router on whole boards: everything connected, nothing too close (DRC), the same answer
//! every time. After OrthoRoute's engine tests (a columnar connector pair joined pad for pad,
//! keep-outs), cuflow's two-layer Lee router (crossing nets meet through vias) and
//! KiCadRoutingTools' corpus of real boards (an ignored test: `CADRS_ROUTER_BOARDS` names a
//! folder of `.kicad_pcb` files, such as its `kicad_files`).

use cadrs_eda::board::{Board, Keepout, PlacedFootprint, Zone, ZoneFill};
use cadrs_eda::drc::{self, Rule};
use cadrs_eda::footprint::FootprintPlacement;
use cadrs_eda::layer::{Layer, LayerSet};
use cadrs_eda::library::LibraryTable;
use cadrs_eda::units::{Pt, mm};
use cadrs_router::{Backend, Directions, Options, apply, route};
use uuid::Uuid;

/// An 0805 resistor at `at` (degrees `angle`) with its pads on `nets`.
fn part(lib: &LibraryTable, at: Pt, angle: f64, nets: [&str; 2]) -> PlacedFootprint {
    let mut fp = lib.footprint("Resistor_SMD:R_0805_2012Metric").expect("0805").clone();
    for p in &mut fp.pads {
        p.net = Some(nets[usize::from(p.number == "2")].to_string());
    }
    PlacedFootprint { id: Uuid::new_v4(), footprint: fp, placement: FootprintPlacement { at, angle, side: Default::default() }, locked: false, symbol: None }
}

/// A `w` × `h` mm board holding `parts`.
fn board(w: f64, h: f64, parts: Vec<PlacedFootprint>) -> Board {
    let mut b = Board::default();
    cadrs_eda::outline::add_rect(&mut b, Pt::mm(0.0, 0.0), Pt::mm(w, h));
    let mut nets: Vec<String> = parts.iter().flat_map(|f| f.footprint.pads.iter().filter_map(|p| p.net.clone())).collect();
    nets.sort();
    nets.dedup();
    b.nets = nets;
    b.footprints = parts;
    b
}

fn cpu() -> Options {
    Options { backend: Backend::Cpu, ..Default::default() }
}

/// The copper rules a route could break.
fn copper_errors(b: &Board) -> Vec<drc::Violation> {
    let rules = [Rule::Clearance, Rule::HoleClearance, Rule::EdgeClearance, Rule::KeepoutViolation, Rule::HoleToHole];
    drc::check(b).violations.into_iter().filter(|v| rules.contains(&v.rule)).collect()
}

/// Routes `b` and checks it: every net connected, no copper rule broken.
fn routed(mut b: Board, opts: &Options) -> (Board, cadrs_router::Outcome) {
    let out = route(&b, opts);
    apply(&mut b, &out);
    assert!(out.unrouted.is_empty(), "unrouted {:?} (conflicts {:?})", out.unrouted, out.conflicts);
    assert_eq!(cadrs_eda::copper::ratsnest(&b), vec![], "still unconnected");
    let errors = copper_errors(&b);
    assert!(errors.is_empty(), "{errors:#?}");
    (b, out)
}

#[test]
fn two_pads_are_joined() {
    let lib = LibraryTable::builtin();
    let b = board(30.0, 20.0, vec![part(&lib, Pt::mm(8.0, 10.0), 0.0, ["A", "B"]), part(&lib, Pt::mm(22.0, 10.0), 0.0, ["A", "C"])]);
    let (b, out) = routed(b, &cpu());
    assert!(!out.tracks.is_empty());
    assert!(b.tracks.iter().all(|t| t.net == "A"));
}

/// A keep-out over `ring` on `layers` forbidding tracks and vias.
fn keepout(ring: Vec<Pt>, layers: &[Layer]) -> Zone {
    Zone {
        id: Uuid::new_v4(),
        name: String::new(),
        net: String::new(),
        layers: LayerSet::of(layers),
        priority: 0,
        outline: vec![ring],
        fill: ZoneFill::default(),
        keepout: Some(Keepout { tracks: true, vias: true, ..Default::default() }),
        locked: false,
        filled: vec![],
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
    vec![Pt::mm(x0, y0), Pt::mm(x1, y0), Pt::mm(x1, y1), Pt::mm(x0, y1)]
}

#[test]
fn crossing_nets_meet_through_vias_and_cannot_on_one_layer() {
    // A left to right, B bottom to top, with keep-outs from each part to the board's edge so
    // neither can go round the other: on two layers one dives under the other (cuflow's two-layer
    // router); with tracks kept off the bottom layer, not both.
    let lib = LibraryTable::builtin();
    let make = || {
        let mut b = board(
            40.0,
            30.0,
            vec![
                // Each net on its parts' inner pads.
                part(&lib, Pt::mm(4.0, 15.0), 0.0, ["X1", "A"]),
                part(&lib, Pt::mm(36.0, 15.0), 0.0, ["A", "X2"]),
                // B on each part's inner pad.
                part(&lib, Pt::mm(20.0, 6.0), 90.0, ["X3", "B"]),
                part(&lib, Pt::mm(20.0, 24.0), 90.0, ["B", "X4"]),
            ],
        );
        let both = [Layer::TopCopper, Layer::BottomCopper];
        // From each part to its edge, up to the part's middle: no way round its end, nor
        // between its pads.
        b.zones.push(keepout(rect(18.5, -1.0, 21.5, 6.0), &both));
        b.zones.push(keepout(rect(18.5, 24.0, 21.5, 31.0), &both));
        b.zones.push(keepout(rect(-1.0, 13.5, 4.0, 16.5), &both));
        b.zones.push(keepout(rect(36.0, 13.5, 41.0, 16.5), &both));
        b
    };
    let (_, out) = routed(make(), &Options { directions: Directions::Alternate, ..cpu() });
    assert!(!out.vias.is_empty(), "two crossing nets on two layers need a via");
    // One layer: a track keep-out over all of the bottom copper.
    let mut b = make();
    b.zones.push(keepout(rect(-1.0, -1.0, 41.0, 31.0), &[Layer::BottomCopper]));
    let out = route(&b, &cpu());
    assert!(out.vias.is_empty() && out.tracks.iter().all(|t| t.layer == Layer::TopCopper));
    assert!(!out.unrouted.is_empty(), "two crossing nets on one layer can't both be routed: {out:?}");
}

#[test]
fn a_keepout_is_routed_round() {
    let lib = LibraryTable::builtin();
    let mut b = board(40.0, 30.0, vec![part(&lib, Pt::mm(5.0, 15.0), 0.0, ["A", "B"]), part(&lib, Pt::mm(35.0, 15.0), 0.0, ["A", "C"])]);
    // Across the straight way, on both layers, tracks and vias.
    b.zones.push(keepout(rect(15.0, 5.0, 25.0, 25.0), &[Layer::TopCopper, Layer::BottomCopper]));
    let (b, _) = routed(b, &cpu());
    for t in &b.tracks {
        for p in [t.a, t.b] {
            assert!(!(p.x > mm(15.0) && p.x < mm(25.0) && p.y > mm(5.0) && p.y < mm(25.0)), "{t:?} inside the keep-out");
        }
    }
}

#[test]
fn columnar_connectors_joined_pad_for_pad() {
    // OrthoRoute's engine smoke board, in small: two rows of parts facing each other, each
    // pad joined to the one opposite, so the routes run side by side and must negotiate.
    let lib = LibraryTable::builtin();
    let mut parts = vec![];
    for k in 0..6 {
        let x = 6.0 + 4.0 * k as f64;
        parts.push(part(&lib, Pt::mm(x, 6.0), 90.0, [&format!("N{k}a"), &format!("N{k}b")]));
        parts.push(part(&lib, Pt::mm(x + 2.0, 24.0), 90.0, [&format!("N{k}b"), &format!("N{k}a")]));
    }
    let (_, out) = routed(board(34.0, 30.0, parts), &cpu());
    assert!(out.conflicts.is_empty());
}

#[test]
fn the_same_board_routes_the_same_every_time() {
    // KiCadRoutingTools #457: no answer may hang on hash or iteration order.
    let lib = LibraryTable::builtin();
    let parts = || {
        let mut v = vec![];
        for k in 0..4 {
            v.push(part(&lib, Pt::mm(5.0 + 6.0 * k as f64, 5.0), 0.0, [&format!("N{k}"), "GND"]));
            v.push(part(&lib, Pt::mm(23.0 - 6.0 * k as f64, 20.0), 0.0, [&format!("N{k}"), "GND"]));
        }
        v
    };
    let key = |o: &cadrs_router::Outcome| {
        let mut t: Vec<_> = o.tracks.iter().map(|t| (t.net.clone(), t.layer, t.a, t.b)).collect();
        t.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        let mut v: Vec<_> = o.vias.iter().map(|v| (v.net.clone(), v.at)).collect();
        v.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        (t, v)
    };
    let a = route(&board(30.0, 25.0, parts()), &cpu());
    let b = route(&board(30.0, 25.0, parts()), &cpu());
    assert!(!a.tracks.is_empty());
    assert_eq!(key(&a), key(&b));
    assert_eq!((a.unrouted, a.passes), (b.unrouted, b.passes));
}

#[test]
fn the_getting_started_board_routes_clean() {
    let lib = LibraryTable::builtin();
    let d = cadrs_eda::getting_started::gs16(&lib);
    assert!(!cadrs_eda::copper::ratsnest(&d.board).is_empty(), "the course board starts unrouted");
    routed(d.board, &cpu());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "needs a GPU and the shader built (cargo gpu build ...)"]
fn the_gpu_routes_as_the_cpu_does() {
    let lib = LibraryTable::builtin();
    let t = std::time::Instant::now();
    let made = cadrs_router::gpu::Gpu::shared();
    eprintln!("device: {:.3} s", t.elapsed().as_secs_f64());
    let gpu = match made {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    };
    eprintln!("on {}", gpu.name);
    let d = cadrs_eda::getting_started::gs16(&lib);
    let t = std::time::Instant::now();
    let cpu_out = route(&d.board, &cpu());
    eprintln!("cpu: {:.3} s, {} passes", t.elapsed().as_secs_f64(), cpu_out.passes);
    let t = std::time::Instant::now();
    let (_, out) = routed(d.board.clone(), &Options { backend: Backend::Gpu, ..Default::default() });
    eprintln!("gpu: {:.3} s, {} passes", t.elapsed().as_secs_f64(), out.passes);
    assert!(out.gpu.is_some());
}

/// KiCadRoutingTools' corpus (or any folder of `.kicad_pcb` files): each board's tracks and
/// vias taken up, routed again, and the share connected and the copper errors reported.
#[test]
#[ignore = "needs CADRS_ROUTER_BOARDS: a folder of .kicad_pcb files"]
fn real_boards() {
    let Some(dir) = std::env::var_os("CADRS_ROUTER_BOARDS") else { return };
    let mut paths: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "kicad_pcb")).collect();
    paths.sort();
    for path in paths {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut warnings = vec![];
        let Ok(mut b) = cadrs_kicad::board::read_board(&text, &mut warnings) else {
            eprintln!("{}: unreadable", path.display());
            continue;
        };
        b.tracks.clear();
        b.vias.clear();
        let before = cadrs_eda::copper::ratsnest(&b).len();
        // The board's own errors (pads too near the edge or each other) are not the router's.
        let had = copper_errors(&b).len();
        let t = std::time::Instant::now();
        let out = route(&b, &Options { backend: std::env::var("CADRS_ROUTER_BACKEND").map_or(Backend::Auto, |v| if v == "cpu" { Backend::Cpu } else { Backend::Gpu }), ..Default::default() });
        let took = t.elapsed();
        apply(&mut b, &out);
        let after = cadrs_eda::copper::ratsnest(&b).len();
        if std::env::var_os("CADRS_ROUTER_TRACE").is_some() {
            eprintln!("  unrouted {:?}, in conflict {:?}", out.unrouted, out.conflicts);
        }
        let found = copper_errors(&b);
        if std::env::var_os("CADRS_ROUTER_TRACE").is_some() {
            for v in &found {
                eprintln!("  {:?} {} {:?} at {:?}", v.rule, v.message, v.items, v.at);
            }
        }
        let errors = found.len().saturating_sub(had);
        eprintln!(
            "{}: {} of {before} connections made, {errors} new copper errors, {} passes, {:.1} s{}",
            path.file_name().unwrap().to_string_lossy(),
            before - after.min(before),
            out.passes,
            took.as_secs_f64(),
            out.gpu.map(|g| format!(" on {g}")).unwrap_or_default()
        );
    }
}
