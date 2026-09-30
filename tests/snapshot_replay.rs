use dashr::settings::{Settings, Uniforms};
use glam::Mat4;
#[test]
fn default_sun_elevation_matches_reference_and_is_configurable() {
    let mut s = Settings::default();
    let sun = s.uniforms(false).sun_atlas;
    assert!((sun[1] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    s.sun_elevation = 1.;
    assert_eq!(&s.uniforms(false).sun_atlas[..3], &[0., 1., 0.]);
    s.sun_elevation = -0.1;
    assert!(s.validate().is_err());
}
#[test]
fn exact_reference_matrix_pose_and_sun_override_is_replayed() {
    let s = Settings::default();
    let mut u = s.uniforms(false);
    u.camera_from_object = Mat4::from_translation(glam::Vec3::new(4., -2., 7.)).to_cols_array_2d();
    u.object_from_camera = Mat4::from_translation(glam::Vec3::new(-4., 2., -7.)).to_cols_array_2d();
    u.projection[0][0] = 2.25;
    u.bones[3] = Mat4::from_rotation_z(0.5).to_cols_array_2d();
    u.sun_atlas = [0., 1., 0., s.atlas as f32];
    let mut replay = Settings {
        uniform_override: Some(u),
        ..s
    };
    let actual = replay.uniforms(true);
    assert_eq!(actual.camera_from_object, u.camera_from_object);
    assert_eq!(actual.object_from_camera, u.object_from_camera);
    assert_eq!(actual.projection, u.projection);
    assert_eq!(actual.bones, u.bones);
    assert_eq!(actual.sun_atlas, u.sun_atlas);
    assert_eq!(actual.control[3], 1);
    replay.step_size = 0.035;
    assert_eq!(replay.uniforms(false).height_step[2], 0.035);
    assert_eq!(replay.uniforms(false).bones, u.bones);
    replay.uniform_override.as_mut().unwrap().bones[1][2][3] = f32::NAN;
    assert!(replay.validate().is_err());
}
#[test]
fn malformed_override_layout_is_rejected_by_serde() {
    assert!(serde_json::from_str::<Uniforms>("{}").is_err());
}
#[test]
fn cpp_snapshot_fixture_preserves_columns_and_failed_snapshots_are_rejected() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let exe = root.join("out/reference-snapshot-test");
    std::fs::create_dir_all(root.join("out")).unwrap();
    let cxx = std::env::var("CXX").unwrap_or_else(|_| "c++".into());
    let status = std::process::Command::new(cxx)
        .current_dir(root)
        .args(["-std=c++17", "tests/reference_snapshot.cpp", "-o"])
        .arg(&exe)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(std::process::Command::new(&exe).status().unwrap().success());
    let output = std::process::Command::new(&exe)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let path = root.join("out/reference-snapshot-fixture.json");
    std::fs::write(&path, &output.stdout).unwrap();
    let settings = Settings::load(&path).unwrap();
    let u = settings.uniforms(false);
    assert_eq!(
        u.camera_from_object,
        [
            [1., 0., 0., 0.],
            [0.25, 1., 0., 0.],
            [0., 0., 1., 0.],
            [4., -2., 7., 1.]
        ]
    );
    assert_eq!(
        u.bones[3],
        [
            [0., 1., 0., 0.],
            [-1., 0., 0., 0.],
            [0., 0., 1., 0.],
            [2., 3., 4., 1.]
        ]
    );
    let mut value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    value["status"] = serde_json::json!("failed");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Settings::load(&path).is_err());
}
