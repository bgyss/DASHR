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
