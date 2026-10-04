#[test]
fn all_resource_variants_validate_with_wgsl_rules() {
    for pass in ["deform", "edgefill", "inverse", "trace"] {
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

#[test]
fn compute_edgefill_shader_validates_with_wgsl_rules() {
    for filtered in [false, true] {
        let source = dashr::shaders::edgefill_compute(filtered);
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| {
            panic!(
                "compute edgefill filtered={filtered}: {}",
                e.emit_to_string(&source)
            )
        });
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("compute edgefill: {e:?}"));
        assert_eq!(module.entry_points.len(), 3);
    }
}

#[test]
fn space_warp_sdf_probe_validates_with_wgsl_rules() {
    let source = dashr::shaders::space_warp_sdf_probe();
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("space warp SDF probe: {}", e.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("space warp SDF probe: {e:?}"));
    assert_eq!(module.entry_points.len(), 1);
}

#[test]
fn inverse_lbs_sdf_probe_validates_with_wgsl_rules() {
    let source = dashr::shaders::inverse_lbs_sdf_probe();
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("inverse LBS SDF probe: {}", e.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("inverse LBS SDF probe: {e:?}"));
    assert_eq!(module.entry_points.len(), 1);
}

#[test]
fn menger_sdf_probe_validates_with_wgsl_rules() {
    let source = dashr::shaders::menger_sdf_probe();
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("Menger SDF probe: {}", e.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("Menger SDF probe: {e:?}"));
    assert_eq!(module.entry_points.len(), 3);
    assert_eq!(
        module
            .entry_points
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        ["cs_flat", "cs_hierarchical", "cs_hierarchical_depth4"].into()
    );
}

#[test]
fn inverse_lbs_voxel_probe_validates_with_wgsl_rules() {
    let source = dashr::shaders::inverse_lbs_voxel_probe();
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("inverse LBS voxel probe: {}", e.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("inverse LBS voxel probe: {e:?}"));
    assert_eq!(module.entry_points.len(), 1);
}

#[test]
fn gaussian_splat_probe_validates_with_wgsl_rules() {
    let source = dashr::shaders::gaussian_splat_probe();
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("Gaussian splat probe: {}", e.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("Gaussian splat probe: {e:?}"));
    assert_eq!(module.entry_points.len(), 2);
    assert_eq!(
        module
            .entry_points
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        ["cs_main", "cs_sort"].into()
    );
}

#[test]
fn production_wgsl_translates_to_hlsl_and_msl_with_naga() {
    let sources = [
        ("deform", dashr::shaders::source("deform", true, 4, 0)),
        ("edgefill", dashr::shaders::source("edgefill", true, 4, 0)),
        ("inverse", dashr::shaders::source("inverse", true, 3, 0)),
        ("trace", dashr::shaders::source("trace", true, 4, 0)),
        ("edgefill_compute", dashr::shaders::edgefill_compute(true)),
    ];
    for (name, source) in sources {
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{name} WGSL parse: {}", e.emit_to_string(&source)));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name} WGSL validation: {e:?}"));

        let msl = naga::back::msl::write_string(
            &module,
            &info,
            &naga::back::msl::Options::default(),
            &naga::back::msl::PipelineOptions::default(),
        )
        .unwrap_or_else(|e| panic!("{name} MSL translation: {e:?}"));
        assert!(!msl.0.is_empty(), "{name} MSL translation is empty");

        let mut hlsl_source = String::new();
        let hlsl_options = naga::back::hlsl::Options::default();
        let hlsl_pipeline = naga::back::hlsl::PipelineOptions::default();
        let mut writer =
            naga::back::hlsl::Writer::new(&mut hlsl_source, &hlsl_options, &hlsl_pipeline);
        writer
            .write(&module, &info, None)
            .unwrap_or_else(|e| panic!("{name} HLSL translation: {e:?}"));
        assert!(!hlsl_source.is_empty(), "{name} HLSL translation is empty");
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
