//! A native board (one with a [`cadrs_eda::Design`]) as a STEP model, as KiCad's File → Export
//! → STEP: the board body (outline, cut-outs, holes) and each footprint's 3D model in place —
//! its model file when the document has it (STEP as is, VRML as a meshed solid), else its
//! generated body ([`cadrs_eda::model3d`]: boxes and cylinders as extruded solids). Footprints
//! without a visible model are left out, as in KiCad.

use cadrs_kernel::{BodyId, Kernel, Motion};

use crate::board::PcbBoard;

fn moved(k: &mut dyn Kernel, b: BodyId, m: &Motion) -> Result<BodyId, String> {
    let r = k.transform_motion(b, m).map_err(|e| e.to_string())?;
    k.release(b);
    r.bodies.into_iter().next().ok_or_else(|| "no body".to_string())
}

/// Bodies with their names (the board, then each part by reference).
pub type NamedBodies = Vec<(String, BodyId)>;

/// The board and its parts' models as bodies (named), with warnings for models that couldn't
/// be made. The caller releases the bodies. The board and the placements are the Part Studio's
/// ([`cadrs_core::pcb::board3d::board_body`], [`cadrs_core::pcb::model::footprint_motion`]), so
/// an exported board and a derived one agree.
pub fn native_board_bodies(k: &mut dyn Kernel, pcb: &PcbBoard, design: &cadrs_eda::Design) -> Result<(NamedBodies, Vec<String>), String> {
    let (board, _, mut warnings) = cadrs_core::pcb::board3d::board_body(k, &design.board)?;
    let mut out = vec![(format!("Board [{}]", pcb.name()), board)];
    let t = cadrs_core::pcb::board3d::thickness(&design.board);
    for f in &design.board.footprints {
        let Some(m) = cadrs_core::pcb::model::shown_model(&f.footprint) else { continue };
        let place = cadrs_core::pcb::model::footprint_motion(&f.placement, t);
        match cadrs_core::pcb::model_solids::model_bodies(k, m) {
            Ok((bodies, unit)) => {
                let motion = crate::mesh::model_motion(m, unit).then(&place);
                for (i, b) in bodies.into_iter().enumerate() {
                    match moved(k, b, &motion) {
                        Ok(b) => out.push((if i == 0 { f.reference().to_string() } else { format!("{} {}", f.reference(), i + 1) }, b)),
                        Err(e) => warnings.push(format!("{}: {e}", f.reference())),
                    }
                }
            }
            Err(e) => warnings.push(format!("{}: {e}", f.reference())),
        }
    }
    Ok((out, warnings))
}

/// The board as STEP bytes (and the warnings).
#[cfg(feature = "occt")]
pub fn native_board_step(pcb: &PcbBoard, design: &cadrs_eda::Design) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut k = cadrs_kernel::backend::occt::OcctKernel::new();
    let (bodies, warnings) = native_board_bodies(&mut k, pcb, design)?;
    let ids: Vec<BodyId> = bodies.iter().map(|(_, b)| *b).collect();
    let bytes = k.export_step(&ids).map_err(|e| e.to_string());
    for b in ids {
        k.release(b);
    }
    Ok((bytes?, warnings))
}

#[cfg(all(test, feature = "occt"))]
mod tests {
    use super::*;

    #[test]
    fn a_course_board_exports_with_its_parts() {
        let lib = cadrs_eda::library::LibraryTable::builtin();
        let d = cadrs_eda::getting_started::gs18(&lib);
        let pcb = cadrs_core::pcb::design::pcb_board("course", &d);
        let mut k = cadrs_kernel::backend::occt::OcctKernel::new();
        let (bodies, warnings) = native_board_bodies(&mut k, &pcb, &d).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        // The board and the three parts' generated bodies (several primitives each).
        assert!(bodies.iter().any(|(n, _)| n.starts_with("Board")), "{:?}", bodies.iter().map(|b| &b.0).collect::<Vec<_>>());
        for r in ["BT1", "R1", "D1"] {
            assert!(bodies.iter().any(|(n, _)| n == r), "{r}");
        }
        // The LED stands on the board's top face (1.6 mm up), not below it.
        let led = bodies.iter().find(|(n, _)| n == "D1").unwrap().1;
        let bb = k.bounding_box(led).unwrap();
        assert!(bb.max.z > 1.6 + 4.0, "{bb:?}");
        let ids: Vec<BodyId> = bodies.iter().map(|b| b.1).collect();
        let step = k.export_step(&ids).unwrap();
        assert!(String::from_utf8_lossy(&step[..200.min(step.len())]).contains("ISO-10303-21"));
    }
}
