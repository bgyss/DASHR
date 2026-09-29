use dashr::{capture::build_provenance, material::hash};
#[test]
fn build_snapshot_identifies_the_compiled_shader() {
    let snapshot = build_provenance();
    assert!(
        snapshot["source_hashes"].is_object(),
        "compiled source snapshot is missing"
    );
    assert_eq!(
        snapshot["source_hashes"]["shaders/trace.wgsl"],
        hash(include_bytes!("../shaders/trace.wgsl"))
    );
    assert!(snapshot["rustc"].as_str().unwrap().starts_with("rustc "));
}
