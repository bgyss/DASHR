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

// FXC cannot address dynamic matrix/array l-values inside the ray loop and
// tries to unroll the caller. Helpers must have statically addressable accesses.
#[test]
fn fxc_math_helpers_do_not_require_dynamic_loop_unrolling() {
    use naga::{Block, Statement};
    fn loops(block: &Block) -> usize {
        block
            .iter()
            .map(|s| match s {
                Statement::Loop {
                    body, continuing, ..
                } => 1 + loops(body) + loops(continuing),
                Statement::Block(b) => loops(b),
                Statement::If { accept, reject, .. } => loops(accept) + loops(reject),
                Statement::Switch { cases, .. } => cases.iter().map(|c| loops(&c.body)).sum(),
                _ => 0,
            })
            .sum()
    }
    let source = dashr::shaders::source("trace", false, 4, 0);
    let module = naga::front::wgsl::parse_str(&source).unwrap();
    for name in ["inverse4", "surface_position"] {
        let function = module
            .functions
            .iter()
            .map(|(_, f)| f)
            .find(|f| f.name.as_deref() == Some(name))
            .unwrap();
        assert_eq!(
            loops(&function.body),
            0,
            "{name} forces FXC array/matrix unrolling"
        );
    }
}
