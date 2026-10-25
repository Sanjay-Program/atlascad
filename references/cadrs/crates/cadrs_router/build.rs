//! Embeds the router's SPIR-V (rust-gpu's build of `cadrs_router_kernels`, in `shaders/`) as
//! `$OUT_DIR/router.spv`; with none built yet an empty file, and the router runs on the CPU.

use std::path::Path;

fn main() {
    let dir = Path::new("shaders");
    println!("cargo:rerun-if-changed=shaders");
    let mut spv: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "spv")).collect();
    spv.sort();
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("router.spv");
    let bytes = spv.first().and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
    std::fs::write(out, bytes).unwrap();
}
