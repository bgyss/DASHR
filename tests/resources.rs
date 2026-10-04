use dashr::{gpu_resources::*, settings::*};
use glam::Vec4;
#[test]
fn resource_policy_keeps_full_float_at_exactly_64_bytes() {
    assert_eq!(
        select_policy(64, true, false, 0).unwrap(),
        Policy {
            planes: 4,
            filtered: true,
            requested_bytes: 64
        }
    );
    assert_eq!(
        select_policy(128, true, true, 0).unwrap(),
        Policy {
            planes: 4,
            filtered: false,
            requested_bytes: 64
        }
    );
    assert_eq!(
        select_policy(32, false, false, 0).unwrap(),
        Policy {
            planes: 2,
            filtered: false,
            requested_bytes: 32
        }
    );
    assert_eq!(select_policy(16, false, false, 0).unwrap().planes, 1);
    assert!(select_policy(15, false, false, 0).is_err());
    assert!(select_policy(32, true, false, 4).is_err());
}
#[test]
fn uniform_offsets_match_wgsl_and_reverse_z_is_infinite_far() {
    assert_eq!(std::mem::size_of::<Uniforms>(), 544);
    assert_eq!(std::mem::offset_of!(Uniforms, bones), 208);
    assert_eq!(std::mem::offset_of!(Uniforms, sun_atlas), 464);
    assert_eq!(std::mem::offset_of!(Uniforms, control), 528);
    let p = reverse_z(16. / 9., 60., 0.1);
    let near = p * Vec4::new(0., 0., 0.1, 1.);
    let far = p * Vec4::new(0., 0., 1000., 1.);
    assert!((near.z / near.w - 1.).abs() < 1e-6);
    assert!((far.z / far.w - 0.0001).abs() < 1e-7);
}
#[test]
fn settings_reject_invalid_allocation_and_numeric_inputs() {
    let mut s = Settings::default();
    s.validate().unwrap();
    s.atlas = 4096;
    assert!(s.validate().is_err());
    s.atlas = 64;
    s.width = 0;
    assert!(s.validate().is_err());
    s.width = 256;
    s.step_size = f32::NAN;
    assert!(s.validate().is_err());
    s.step_size = 0.02;
    s.step_budget = 0;
    assert!(s.validate().is_err());
    s.step_budget = 10000;
    s.camera = s.target;
    assert!(s.validate().is_err());
}

#[test]
fn hit_refinement_is_opt_in_and_keeps_the_uniform_layout() {
    let mut settings = Settings::default();
    assert_eq!(settings.uniforms(false).modes[3], -1);
    assert_eq!(settings.uniforms(false).control[3], 0);

    settings.hit_refinement = true;
    assert_eq!(settings.uniforms(false).modes[3], -1);
    assert_eq!(settings.uniforms(false).control[3], 2);
    assert_eq!(std::mem::size_of::<Uniforms>(), 544);

    settings.hit_refinement = false;
    settings.seam_aware_stepping = true;
    assert_eq!(settings.uniforms(false).control[3], 4);
    settings.adaptive_steps = true;
    assert_eq!(settings.uniforms(false).control[3], 12);
    settings.hit_refinement = true;
    assert_eq!(settings.uniforms(false).control[3], 14);
    settings.specialized_inverse = true;
    assert_eq!(settings.uniforms(false).control[3], 30);
    settings.stored_inverse = true;
    assert_eq!(settings.uniforms(false).control[3], 62);
    settings.indirect_edgefill = true;
    assert_eq!(settings.uniforms(false).control[3], 126);
    settings.split_teleport = true;
    assert_eq!(settings.uniforms(false).control[3], 254);

    settings.debug_max_steps = 7;
    assert_eq!(settings.uniforms(false).modes[3], 7);
    assert_eq!(settings.uniforms(false).control[3], 254);

    let mut old_settings = serde_json::to_value(Settings::default()).unwrap();
    old_settings
        .as_object_mut()
        .unwrap()
        .remove("hit_refinement");
    let decoded: Settings = serde_json::from_value(old_settings).unwrap();
    assert!(!decoded.hit_refinement);
}

#[test]
fn scale_derived_tolerances_have_uv_and_object_space_units() {
    let mut settings = Settings::default();
    let reference = settings.resolved_tolerances();
    assert_eq!(reference.normal_delta_uv, settings.delta_uv);
    assert_eq!(
        reference.local_shadow_bias_object_units,
        settings.shadow_acne
    );

    settings.epsilon_policy = EpsilonPolicy::ScaleDerived;
    let derived = settings.resolved_tolerances();
    assert!((derived.normal_delta_uv - 1.0 / settings.atlas as f32).abs() < 1e-8);
    assert!((derived.local_shadow_bias_object_units - 0.004).abs() < 1e-7);
    let uniforms = settings.uniforms(false);
    assert_eq!(uniforms.lighting[0], derived.normal_delta_uv);
    assert_eq!(uniforms.lighting[1], derived.local_shadow_bias_object_units);

    let encoded = serde_json::to_value(&settings).unwrap();
    assert_eq!(encoded["epsilon_policy"], "scale_derived");
}
