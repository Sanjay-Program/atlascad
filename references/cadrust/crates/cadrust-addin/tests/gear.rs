//! Runs the shipped gear add-in through the wasmi host.

use cadrust_addin::{Addin, Request};

const WASM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../addins/dist/gear-generator.wasm");

#[test]
fn gear_addin_appends_sketch_and_extrude() {
    let mut addin = Addin::load_file(std::path::Path::new(WASM)).expect("load add-in (run scripts/build-addins.sh)");
    let m = addin.manifest().unwrap();
    assert_eq!(m.id, "gear-generator");
    assert_eq!(m.commands[0].id, "gear");
    let doc = serde_json::json!({ "version": 1, "units": "mm", "parameters": {}, "suppressed": [], "timeline": [] });
    let resp = addin
        .run(&Request { command: "gear", params: serde_json::json!({ "teeth": 16, "module": 2, "thickness": 5 }), document: &doc, selection: vec![] })
        .unwrap();
    let out = resp.document.unwrap();
    let tl = out["timeline"].as_array().unwrap();
    assert_eq!(tl.len(), 2);
    assert_eq!(tl[0]["type"], "Sketch");
    assert_eq!(tl[0]["entities"][0]["points"].as_array().unwrap().len(), 16 * 17);
    assert_eq!(tl[1]["type"], "Extrude");
    assert_eq!(tl[1]["distance"], 5.0);
    // Unknown commands are reported as add-in errors, not host errors.
    let err = addin.run(&Request { command: "nope", params: serde_json::json!({}), document: &doc, selection: vec![] }).unwrap_err();
    assert!(matches!(err, cadrust_addin::Error::Addin(_)));
}
