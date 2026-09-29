#[test]
fn all_resource_variants_validate_with_wgsl_rules() {
    for pass in ["deform", "edgefill", "trace"] {
        for filtered in [false, true] {
            for planes in [1, 2, 4] {
                for first in (0..4).step_by(planes as usize) {
                    let source = dashr::shaders::source(pass, filtered, planes, first);
                    let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| {
                        panic!(
                            "{pass} filtered={filtered} planes={planes}: {}",
                            e.emit_to_string(&source)
                        )
                    });
                    naga::valid::Validator::new(
                        naga::valid::ValidationFlags::all(),
                        naga::valid::Capabilities::all(),
                    )
                    .validate(&module)
                    .unwrap_or_else(|e| panic!("{pass}: {e:?}"));
                    assert_eq!(
                        module.entry_points.len(),
                        2,
                        "{pass} needs vertex and fragment stages"
                    );
                }
            }
        }
    }
}
