//! Imported mesh bodies in a `.zcad` document.
//!
//! The document is the source of truth: these tests check that an import survives serialization,
//! undo/redo and validation, not merely that the setter accepted it.

use cad_contract::ids::DocumentId;
use cad_contract::mesh::Mesh;
use cad_contract::units::Unit;
use cad_document::{PROJECT_SCHEMA, Project};

fn cube() -> Mesh {
    let positions = vec![
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [10.0, 10.0, 0.0],
        [0.0, 10.0, 0.0],
        [0.0, 0.0, 5.0],
        [10.0, 0.0, 5.0],
        [10.0, 10.0, 5.0],
        [0.0, 10.0, 5.0],
    ];
    let triangles = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    Mesh {
        positions,
        triangles,
        normals: Vec::new(),
    }
}

#[test]
fn an_import_is_persisted_undoable_and_removable() -> Result<(), Box<dyn std::error::Error>> {
    let mut project = Project::new("part", DocumentId::new(1), Unit::Millimeter);
    project.add_imported_body("Blob", "STEP", cube())?;
    assert_eq!(project.imports().len(), 1);
    assert_eq!(project.imports()[0].name, "Blob");
    assert_eq!(project.imports()[0].source, "STEP");
    assert_eq!(project.imports()[0].triangles(), 12);
    let volume = project.imports()[0].volume_mm3();
    assert!((volume - 500.0).abs() < 1e-9, "volume was {volume}");

    let json = project.to_json()?;
    let loaded = Project::from_json(json.as_bytes())?;
    assert_eq!(loaded.schema, PROJECT_SCHEMA);
    assert_eq!(loaded.imports().len(), 1);
    assert_eq!(loaded.imports()[0].name, "Blob");
    assert_eq!(loaded.imports()[0].mesh.triangle_count(), 12);

    project.undo()?;
    assert!(project.imports().is_empty());
    project.redo()?;
    assert_eq!(project.imports().len(), 1);

    project.remove_imported_body("blob")?;
    assert!(project.imports().is_empty());
    Ok(())
}

#[test]
fn duplicate_names_and_empty_meshes_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let mut project = Project::new("part", DocumentId::new(1), Unit::Millimeter);
    project.add_imported_body("Blob", "STEP", cube())?;
    assert!(project.add_imported_body("blob", "STEP", cube()).is_err());
    let empty = Mesh {
        positions: vec![],
        triangles: vec![],
        normals: vec![],
    };
    assert!(project.add_imported_body("Empty", "STEP", empty).is_err());
    assert!(project.add_imported_body("   ", "STEP", cube()).is_err());
    Ok(())
}

#[test]
fn a_schema_nine_file_migrates_with_no_imports() -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new("part", DocumentId::new(1), Unit::Millimeter);
    let mut value: serde_json::Value = serde_json::from_str(&project.to_json()?)?;
    value["schema"] = serde_json::json!(9);
    let migrated = Project::from_json(&serde_json::to_vec(&value)?)?;
    assert_eq!(migrated.schema, PROJECT_SCHEMA);
    assert!(migrated.imports().is_empty());
    Ok(())
}
