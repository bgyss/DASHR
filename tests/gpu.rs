use dashr::{asset::MeshKind, passes::Renderer, settings::Settings};
use std::path::Path;
#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn actual_device_full_float_fallbacks_and_trace_states() {
    let mut baseline: Option<Vec<Vec<[f32; 4]>>> = None;
    let mut baseline_atlas: Option<Vec<Vec<[f32; 4]>>> = None;
    for (manual, planes) in [(false, 0), (true, 2), (true, 1)] {
        let s = Settings {
            mesh: MeshKind::Cube,
            atlas: 64,
            width: 80,
            height: 64,
            texture_set: 3,
            time: 0.,
            animation_amount: 0.,
            ..Settings::default()
        };
        let mut renderer = pollster::block_on(Renderer::headless(
            &s,
            Path::new("demo/assets"),
            manual,
            planes,
        ))
        .unwrap();
        let atlas = renderer.atlas_planes().unwrap();
        assert_eq!(atlas.len(), 4);
        assert!(atlas.iter().flatten().flatten().all(|v| v.is_finite()));
        let frame = renderer.render_capture(&s).unwrap();
        assert_eq!(frame.len(), 4);
        assert!(frame.iter().flatten().flatten().all(|v| v.is_finite()));
        assert!(
            frame[2]
                .iter()
                .chain(&frame[3])
                .all(|p| p[0] == p[0].round()),
            "diagnostic background must use exact not-launched status zero"
        );
        let hits = frame[2].iter().filter(|p| p[0] == 1.).count();
        assert!(hits > 80, "expected actual traced cube hits; got {hits}");
        assert!(frame[3].iter().all(|p| p[0] >= 0. && p[0] <= 5.));
        if let Some(reference) = &baseline {
            let disagreement = reference[2]
                .iter()
                .zip(&frame[2])
                .filter(|(a, b)| (a[0] == 1.) != (b[0] == 1.))
                .count();
            assert!(
                disagreement as f64 / (s.width * s.height) as f64 <= 0.001,
                "fallback changed hit mask in {disagreement} pixels"
            );
            let reference = baseline_atlas.as_ref().unwrap();
            let error = reference
                .iter()
                .flatten()
                .flatten()
                .zip(atlas.iter().flatten().flatten())
                .map(|(a, b)| (a - b).abs())
                .fold(0., f32::max);
            assert!(error < 1e-5, "split/manual atlas error {error}");
        } else {
            baseline = Some(frame.clone());
            baseline_atlas = Some(atlas.clone());
        }
        let mut budget = s.clone();
        budget.step_budget = 1;
        let frame = renderer.render_capture(&budget).unwrap();
        assert!(
            frame[2].iter().any(|p| p[0] == 3.),
            "budget exits must be exposed rather than forced hits"
        );
        let mut height_debug = s.clone();
        height_debug.debug = 1;
        height_debug.step_budget = 1;
        let frame = renderer.render_capture(&height_debug).unwrap();
        assert!(
            frame[2].iter().chain(&frame[3]).all(|p| p[0] == 0.),
            "raw height debug must bypass primary and shadow tracing"
        );
    }
}
#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn gpu_uniform_sampler_and_affine_contracts() {
    for manual in [true, false] {
        let ctx = pollster::block_on(dashr::gpu_resources::GpuContext::new(
            dashr::gpu_resources::GpuContext::instance(),
            None,
            manual,
            0,
        ))
        .unwrap();
        let report = dashr::probe::contracts(&ctx).unwrap();
        assert!(
            report.get("flat_trace").is_some(),
            "flat chart trace/depth oracle is missing"
        );
    }
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn frozen_snapshot_replay_preserves_pixels_and_composites_background() {
    let settings = Settings {
        atlas: 64,
        width: 80,
        height: 64,
        texture_set: 3,
        background: [0.45, 0.55, 0.60],
        ..Settings::default()
    };
    let mut renderer = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let expected = renderer.render_capture(&settings).unwrap();
    let replay = Settings {
        uniform_override: Some(settings.uniforms(false)),
        camera: [4., 2., -9.],
        time: 27.,
        sun_time: 3.,
        ..settings.clone()
    };
    replay.validate().unwrap();
    assert_eq!(expected, renderer.render_capture(&replay).unwrap());
    let misses: Vec<_> = expected[0].iter().filter(|p| p[3] == 0.).collect();
    assert!(!misses.is_empty());
    assert!(misses.iter().all(|p| p[..3] == settings.background));
}
