#![cfg(feature = "tooling")]

use dashr::{
    asset::MeshKind,
    passes::Renderer,
    settings::{EpsilonPolicy, Settings},
};
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
            report.get("nonlinear_height_trace").is_some(),
            "U2 nonlinear heightfield CPU-root probe is missing"
        );
        let nonlinear = &report["nonlinear_height_trace"];
        let hardware_baseline = &nonlinear["baseline"];
        let hardware_refined = &nonlinear["refined"];
        let nonlinear_manual_baseline = &nonlinear["manual_bilinear"]["baseline"];
        let nonlinear_manual_refined = &nonlinear["manual_bilinear"]["refined"];
        let hardware_baseline_error = hardware_baseline["cpu_root_distance_max_abs_error"]
            .as_f64()
            .unwrap();
        let hardware_refined_error = hardware_refined["cpu_root_distance_max_abs_error"]
            .as_f64()
            .unwrap();
        assert!(
            hardware_refined_error < hardware_baseline_error,
            "U2 hardware-sampled refinement did not improve nonlinear CPU-root distance error: {hardware_baseline_error} -> {hardware_refined_error}"
        );
        assert!(
            hardware_refined_error < 1e-4,
            "U2 hardware-sampled refined root error {hardware_refined_error} exceeds the 1e-4 distance budget"
        );
        assert_eq!(
            nonlinear_manual_refined["cpu_root_verified"], true,
            "U2 refinement result must carry independent nonlinear CPU-root evidence"
        );
        assert_eq!(
            nonlinear_manual_refined["within_cpu_root_gate"], true,
            "U2 refined result must pass the independent CPU-root distance gate"
        );
        let baseline_root_error = nonlinear_manual_baseline["cpu_root_distance_max_abs_error"]
            .as_f64()
            .unwrap();
        let refined_root_error = nonlinear_manual_refined["cpu_root_distance_max_abs_error"]
            .as_f64()
            .unwrap();
        assert!(
            refined_root_error < baseline_root_error,
            "U2 binary refinement did not improve nonlinear CPU-root distance error: {baseline_root_error} -> {refined_root_error}"
        );
        assert!(
            refined_root_error < 1e-4,
            "U2 refined nonlinear root error {refined_root_error} exceeds the 1e-4 distance budget"
        );
        let seam_teleport = nonlinear
            .get("seam_teleport")
            .expect("U2 seam-teleport CPU-root probe is missing");
        let baseline_teleport = &seam_teleport["baseline"];
        let refined_teleport = &seam_teleport["refined"];
        assert_eq!(baseline_teleport["cpu_root_verified"], true);
        assert_eq!(refined_teleport["cpu_root_verified"], true);
        assert_eq!(refined_teleport["within_cpu_root_gate"], true);
        let baseline_seam_samples = baseline_teleport["seam_root_samples"].as_u64().unwrap();
        assert!(
            baseline_seam_samples >= 16,
            "U2 seam oracle covered only {baseline_seam_samples} crossings"
        );
        assert_eq!(refined_teleport["seam_root_samples"], baseline_seam_samples);
        let baseline_seam_error = baseline_teleport["cpu_root_distance_max_abs_error"]
            .as_f64()
            .unwrap();
        let refined_seam_error = refined_teleport["cpu_root_distance_max_abs_error"]
            .as_f64()
            .unwrap();
        assert!(
            refined_seam_error < baseline_seam_error,
            "U2 seam refinement did not improve nonlinear CPU-root distance error: {baseline_seam_error} -> {refined_seam_error}"
        );
        assert!(
            refined_seam_error < 1e-4,
            "U2 refined seam-root error {refined_seam_error} exceeds the 1e-4 distance budget"
        );
        let sdf_predicted_step = seam_teleport
            .get("sdf_predicted_step")
            .expect("U3 SDF-predicted seam-step CPU-root probe is missing");
        assert_eq!(sdf_predicted_step["cpu_root_verified"], true);
        assert_eq!(
            sdf_predicted_step["seam_root_samples"],
            baseline_seam_samples
        );
        let predicted_step_error = sdf_predicted_step["cpu_root_distance_max_abs_error"]
            .as_f64()
            .unwrap();
        assert!(
            predicted_step_error < baseline_seam_error * 0.1,
            "U3 predicted seam stepping did not reduce the seam-root error by 10x: {baseline_seam_error} -> {predicted_step_error}"
        );
        let nominal_step = sdf_predicted_step["step_size"].as_f64().unwrap();
        assert!(
            predicted_step_error < nominal_step,
            "U3 predicted seam stepping exceeded one nominal ray step ({nominal_step}): {predicted_step_error}"
        );
        let inverse_transform_retry = nonlinear
            .get("inverse_transform_retry")
            .expect("U4 inverse-transform retry probe is missing");
        let baseline_retry = &inverse_transform_retry["baseline"];
        let adaptive_retry = &inverse_transform_retry["adaptive"];
        assert_eq!(adaptive_retry["cpu_transform_reference_verified"], true);
        let initial_transform_change = adaptive_retry["initial_candidate_relative_change"]
            .as_f64()
            .unwrap();
        let baseline_accepted_change = baseline_retry["accepted_candidate_relative_change"]
            .as_f64()
            .unwrap();
        let adaptive_accepted_change = adaptive_retry["accepted_candidate_relative_change"]
            .as_f64()
            .unwrap();
        let baseline_step = baseline_retry["final_step_size"].as_f64().unwrap();
        let adaptive_step = adaptive_retry["final_step_size"].as_f64().unwrap();
        assert!(
            initial_transform_change > 0.25,
            "U4 fixture did not trigger the 0.25 transform-change threshold: {initial_transform_change}"
        );
        assert!(
            baseline_accepted_change > 0.25,
            "U4 baseline unexpectedly changed its candidate step: {baseline_accepted_change}"
        );
        assert!(
            adaptive_accepted_change <= 0.25,
            "U4 retries accepted a candidate above the transform-change threshold: {adaptive_accepted_change}"
        );
        assert!(
            adaptive_step < baseline_step,
            "U4 retries did not shorten the candidate step: {baseline_step} -> {adaptive_step}"
        );
        println!(
            "U2-U4 root/step metrics (float maps={}): hit hardware={hardware_baseline_error}->{hardware_refined_error}, hit manual={baseline_root_error}->{refined_root_error}, seam={baseline_seam_error}->{refined_seam_error}, predicted-step={baseline_seam_error}->{predicted_step_error} across {baseline_seam_samples} crossings, inverse retry step={baseline_step}->{adaptive_step} and accepted transform change={baseline_accepted_change}->{adaptive_accepted_change}",
            if manual { "manual" } else { "hardware" }
        );
        assert!(
            report.get("flat_trace").is_some(),
            "flat chart trace/depth oracle is missing"
        );
        let ramp = &report["flat_ramp_trace"];
        let baseline_ramp = &ramp["baseline"];
        assert_eq!(baseline_ramp["height_profile"], "horizontal_ramp");
        assert_eq!(baseline_ramp["verified_against_analytic_plane"], true);
        assert!(baseline_ramp["within_accuracy_gate"].as_bool().is_some());
        assert!(baseline_ramp["max_abs_error"].as_f64().unwrap().is_finite());
        let refined_ramp = &ramp["refined"];
        assert_eq!(refined_ramp["hit_refinement"], true);
        assert_eq!(refined_ramp["verified_against_analytic_plane"], true);
        assert!(refined_ramp["within_accuracy_gate"].as_bool().is_some());
        assert!(refined_ramp["max_abs_error"].as_f64().unwrap().is_finite());
        let manual_refined_ramp = &ramp["manual_bilinear"]["refined"];
        assert_eq!(manual_refined_ramp["manual_height_sampling"], true);
        if manual {
            assert_eq!(manual_refined_ramp["within_accuracy_gate"], true);
        } else {
            assert!(
                manual_refined_ramp["within_accuracy_gate"]
                    .as_bool()
                    .is_some()
            );
        }
        assert!(
            report["flat_trace_refined"]["hit_refinement"] == true,
            "same-chart hit refinement oracle is missing"
        );
        assert!(
            report["flat_trace_specialized"]["specialized_inverse"] == true,
            "specialized inverse oracle is missing"
        );
        assert!(
            report["flat_trace_stored_inverse"]["stored_inverse"] == true,
            "stored inverse oracle is missing"
        );
        assert_eq!(
            report["space_warp_sdf"]["status_disagreements"], 0,
            "GPU warped SDF statuses differ from the CPU root-search oracle"
        );
        assert_eq!(
            report["space_warp_sdf"]["conservative_step_violations"], 0,
            "GPU warped SDF exceeded the CPU surface root"
        );
        let warp_variants = report["space_warp_sdf"]["query_to_canonical_warps"]
            .as_array()
            .unwrap();
        assert_eq!(warp_variants.len(), 2);
        assert!(
            warp_variants[1]["minimum_sampled_jacobian_determinant"]
                .as_f64()
                .unwrap()
                > 0.0,
            "two-transform blend proxy folded in the sampled ray domain"
        );
        for resolution in report["voxel_sdf"]["resolutions"].as_array().unwrap() {
            assert_eq!(
                resolution["conservative_bound_violations"], 0,
                "dense voxel SDF conservative margin failed at {:?}",
                resolution["resolution"]
            );
        }
    }
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn inverse_lbs_sdf_gpu_probe_matches_cpu_truth_across_fixed_pose_sequence() {
    let ctx = pollster::block_on(dashr::gpu_resources::GpuContext::new(
        dashr::gpu_resources::GpuContext::instance(),
        None,
        true,
        0,
    ))
    .unwrap();
    let report = dashr::probe::contracts(&ctx).unwrap();
    let probe = &report["inverse_lbs_sdf"];

    assert_eq!(probe["sequence_pose_count"], 120);
    assert_eq!(probe["rays_per_pose"], 32);
    assert_eq!(probe["inverse_solve_failures"], 0);
    assert_eq!(probe["status_disagreements"], 0);
    assert_eq!(probe["conservative_step_violations"], 0);
    assert!(
        probe["minimum_sampled_jacobian_determinant"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(probe["maximum_hit_distance_error"].as_f64().unwrap() <= 0.005);
    let mesh_baseline = &probe["conventional_skinned_mesh_baseline"];
    assert_eq!(mesh_baseline["pose_count"], 120);
    assert_eq!(mesh_baseline["rays_per_pose"], 32);
    assert!(mesh_baseline["triangle_count"].as_u64().unwrap() > 500);
    assert!(
        mesh_baseline["maximum_analytic_depth_difference"]
            .as_f64()
            .unwrap()
            <= 0.03
    );
    assert_eq!(mesh_baseline["mesh_status_disagreements"], 0);
    let stress = &probe["self_contact_stress_pose"];
    assert!(stress["end_center_clearance_estimate"].as_f64().unwrap() < 0.0);
    assert!(stress["certified_min_singular_value"].as_f64().unwrap() <= 0.0);
    assert_eq!(stress["invalid_ray_statuses"], 32);
    assert!(
        stress["skinned_mesh_self_intersection_pairs"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn finite_menger_sdf_gpu_probe_matches_exact_ray_box_reference() {
    let ctx = pollster::block_on(dashr::gpu_resources::GpuContext::new(
        dashr::gpu_resources::GpuContext::instance(),
        None,
        true,
        0,
    ))
    .unwrap();
    let report = dashr::probe::contracts(&ctx).unwrap();
    let probe = &report["menger_sdf"];

    assert_eq!(probe["depth"], 2);
    assert_eq!(probe["box_count"], 400);
    assert_eq!(probe["ray_count"], 1024);
    assert_eq!(probe["bvh_node_count"], 255);
    assert_eq!(probe["cpu_bvh_distance_check"]["query_count"], 729);
    assert_eq!(
        probe["cpu_bvh_distance_check"]["maximum_distance_error"],
        0.0
    );
    assert_eq!(probe["flat_status_disagreements"], 0);
    assert_eq!(probe["hierarchical_status_disagreements"], 0);
    assert_eq!(probe["flat_conservative_step_violations"], 0);
    assert_eq!(probe["hierarchical_conservative_step_violations"], 0);
    assert_eq!(probe["hierarchical_stack_overflows"], 0);
    assert!(probe["hit_rays"].as_u64().unwrap() > 0);
    assert!(
        probe["hierarchical_maximum_hit_distance_error"]
            .as_f64()
            .unwrap()
            <= 0.001
    );
    assert!(
        probe["hierarchical_average_box_sdf_evaluations_per_distance_query"]
            .as_f64()
            .unwrap()
            < probe["flat_average_box_sdf_evaluations_per_distance_query"]
                .as_f64()
                .unwrap()
    );
    assert!(probe["trace_steps_max"].as_u64().unwrap() < 256);

    let depth_three = &probe["depth3_followup"];
    assert_eq!(depth_three["depth"], 3);
    assert_eq!(depth_three["box_count"], 8000);
    assert_eq!(depth_three["ray_count"], 1024);
    assert_eq!(
        depth_three["cpu_bvh_distance_check"]["maximum_distance_error"],
        0.0
    );
    assert_eq!(depth_three["status_disagreements"], 0);
    assert_eq!(depth_three["conservative_step_violations"], 0);
    assert_eq!(depth_three["stack_overflows"], 0);
    assert!(depth_three["hit_rays"].as_u64().unwrap() > 0);
    assert!(depth_three["maximum_hit_distance_error"].as_f64().unwrap() <= 0.001);
    assert!(
        depth_three["average_box_sdf_evaluations_per_distance_query"]
            .as_f64()
            .unwrap()
            < 8000.0
    );

    let depth_four = &probe["depth4_followup"];
    assert_eq!(depth_four["depth"], 4);
    assert_eq!(depth_four["box_count"], 160000);
    assert_eq!(depth_four["ray_count"], 1024);
    assert_eq!(
        depth_four["cpu_bvh_distance_check"]["maximum_distance_error"],
        0.0
    );
    assert_eq!(
        depth_four["status_disagreements"],
        depth_four["budget_exits"]
    );
    assert_eq!(
        depth_four["budget_exit_expected_misses"],
        depth_four["budget_exits"]
    );
    assert_eq!(depth_four["budget_exit_expected_hits"], 0);
    assert_eq!(depth_four["unexpected_status_disagreements"], 0);
    assert_eq!(depth_four["conservative_step_violations"], 0);
    assert_eq!(depth_four["stack_overflows"], 0);
    assert!(depth_four["hit_rays"].as_u64().unwrap() > 0);
    assert!(
        depth_four["trace_steps_max"].as_u64().unwrap()
            <= depth_four["max_trace_steps"].as_u64().unwrap()
    );
    assert!(depth_four["maximum_hit_distance_error"].as_f64().unwrap() <= 0.001);
    assert!(
        depth_four["average_box_sdf_evaluations_per_distance_query"]
            .as_f64()
            .unwrap()
            < 160000.0
    );
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn inverse_lbs_dense_voxel_sdf_uses_global_lipschitz_bound() {
    let ctx = pollster::block_on(dashr::gpu_resources::GpuContext::new(
        dashr::gpu_resources::GpuContext::instance(),
        None,
        true,
        0,
    ))
    .unwrap();
    let report = dashr::probe::contracts(&ctx).unwrap();
    let probe = &report["inverse_lbs_voxel_sdf"];

    assert_eq!(probe["sequence_pose_count"], 120);
    assert_eq!(probe["resolution"], serde_json::json!([32, 32, 32]));
    assert_eq!(probe["voxel_payload_bytes"], 131072);
    assert_eq!(probe["inverse_solve_failures"], 0);
    assert_eq!(probe["status_disagreements_vs_voxel_cpu"], 0);
    assert_eq!(probe["status_disagreements_vs_analytic"], 0);
    assert_eq!(probe["conservative_step_violations"], 0);
    assert!(
        probe["maximum_hit_distance_error_vs_analytic"]
            .as_f64()
            .unwrap()
            <= 0.02
    );
    assert!(probe["trilinear_lipschitz_bound"].as_f64().unwrap() >= 1.0);

    let higher_resolution = &probe["higher_resolution"];
    assert_eq!(
        higher_resolution["resolution"],
        serde_json::json!([64, 64, 64])
    );
    assert_eq!(higher_resolution["voxel_payload_bytes"], 1_048_576);
    assert_eq!(higher_resolution["inverse_solve_failures"], 0);
    assert_eq!(higher_resolution["status_disagreements_vs_voxel_cpu"], 0);
    assert_eq!(higher_resolution["status_disagreements_vs_analytic"], 0);
    assert_eq!(higher_resolution["conservative_step_violations"], 0);
    assert!(
        higher_resolution["maximum_hit_distance_error_vs_voxel_cpu"]
            .as_f64()
            .unwrap()
            <= 0.001
    );
    assert!(
        higher_resolution["maximum_hit_distance_error_vs_analytic"]
            .as_f64()
            .unwrap()
            < probe["maximum_hit_distance_error_vs_analytic"]
                .as_f64()
                .unwrap()
    );
    assert!(
        higher_resolution["sampled_interpolation_error"]["max_abs_error"]
            .as_f64()
            .unwrap()
            < probe["sampled_interpolation_error"]["max_abs_error"]
                .as_f64()
                .unwrap()
    );
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn gaussian_splat_probe_reports_compositing_and_mesh_handoff() {
    let ctx = pollster::block_on(dashr::gpu_resources::GpuContext::new(
        dashr::gpu_resources::GpuContext::instance(),
        None,
        true,
        0,
    ))
    .unwrap();
    let report = dashr::probe::contracts(&ctx).unwrap();
    let probe = &report["gaussian_splat_adapter"];

    assert_eq!(probe["pose_count"], 2);
    assert_eq!(probe["splat_count"], 256);
    assert_eq!(probe["image_size"], serde_json::json!([32, 32]));
    assert_eq!(probe["sort_order_inversions"], 0);
    assert_eq!(probe["gpu_sorted_order_mismatches"], 0);
    assert_eq!(probe["gpu_sort_order_inversions"], 0);
    assert_eq!(probe["gpu_cpu_output_mismatches"], 0);
    assert!(probe["cpu_gpu_max_color_error"].as_f64().unwrap() <= 1e-3);
    assert!(probe["cpu_gpu_max_coverage_error"].as_f64().unwrap() <= 1e-3);
    assert!(probe["cpu_gpu_max_depth_error"].as_f64().unwrap() <= 1e-3);
    assert!(probe["mesh_hit_pixels"].as_u64().unwrap() > 0);
    assert!(probe["splat_covered_pixels"].as_u64().unwrap() > 0);
    assert_eq!(
        probe["depth_semantics"],
        "opacity-weighted mean primitive center depth; not a unique surface hit"
    );
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

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn opt_in_traversal_refinement_preserves_cube_hit_mask_and_statuses() {
    let settings = Settings {
        mesh: MeshKind::Cube,
        atlas: 64,
        width: 80,
        height: 64,
        texture_set: 3,
        time: 0.0,
        animation_amount: 0.0,
        ..Settings::default()
    };
    let mut reference = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let reference_frame = reference.render_capture(&settings).unwrap();

    let refined_settings = Settings {
        hit_refinement: true,
        seam_aware_stepping: true,
        adaptive_steps: true,
        ..settings.clone()
    };
    let mut refined = pollster::block_on(Renderer::headless(
        &refined_settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let refined_frame = refined.render_capture(&refined_settings).unwrap();

    let disagreements = reference_frame[2]
        .iter()
        .zip(&refined_frame[2])
        .filter(|(reference, refined)| (reference[0] == 1.0) != (refined[0] == 1.0))
        .count();
    assert!(
        disagreements as f64 / (settings.width * settings.height) as f64 <= 0.001,
        "opt-in traversal changed the cube hit mask in {disagreements} pixels"
    );
    for channel in [2, 3] {
        let reference_failures = reference_frame[channel]
            .iter()
            .filter(|trace| trace[0] == 3.0 || trace[0] == 4.0)
            .count();
        let refined_failures = refined_frame[channel]
            .iter()
            .filter(|trace| trace[0] == 3.0 || trace[0] == 4.0)
            .count();
        assert!(
            refined_failures <= reference_failures,
            "traversal refinement added {channel} failures: {reference_failures} -> {refined_failures}"
        );
    }

    let budget_settings = Settings {
        step_budget: 1,
        ..refined_settings
    };
    let budget_frame = refined.render_capture(&budget_settings).unwrap();
    assert!(
        budget_frame[2].iter().any(|trace| trace[0] == 3.0),
        "opt-in refinement must retain explicit budget-exhaustion status"
    );
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn stored_inverse_probe_reports_cube_quality_delta() {
    let settings = Settings {
        mesh: MeshKind::Cube,
        atlas: 64,
        width: 80,
        height: 64,
        texture_set: 3,
        time: 0.0,
        animation_amount: 0.0,
        ..Settings::default()
    };
    let mut reference = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let expected = reference.render_capture(&settings).unwrap();
    let stored_settings = Settings {
        stored_inverse: true,
        ..settings.clone()
    };
    let mut stored = pollster::block_on(Renderer::headless(
        &stored_settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let actual = stored.render_capture(&stored_settings).unwrap();
    let status_disagreements = expected[2]
        .iter()
        .zip(&actual[2])
        .filter(|(a, b)| a[0] != b[0])
        .count();
    assert_eq!(status_disagreements, 0);
    let max_color_error = expected[0]
        .iter()
        .zip(&actual[0])
        .flat_map(|(a, b)| (0..3).map(move |c| (a[c] - b[c]).abs()))
        .fold(0.0f32, f32::max);
    assert!(max_color_error.is_finite());
    println!(
        "U7 stored inverse probe: status disagreements={status_disagreements}, max RGB error={max_color_error}"
    );
    let inverse = stored.stored_inverse_planes().unwrap().unwrap();
    assert_eq!(inverse.len(), 3);
    assert!(
        inverse
            .iter()
            .flatten()
            .flatten()
            .all(|value| value.is_finite())
    );
    let mut runs = Vec::new();
    for run in 0..5 {
        let mut reference_trace = Vec::new();
        let mut reference_total = Vec::new();
        let mut stored_trace = Vec::new();
        let mut stored_total = Vec::new();
        if run % 2 == 0 {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&reference, &settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&stored, &stored_settings);
            }
        } else {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&stored, &stored_settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&reference, &settings);
            }
        }
        for sample in 0..300 {
            if (run + sample) % 2 == 0 {
                record_gpu_timing(
                    &reference,
                    &settings,
                    &mut reference_trace,
                    &mut reference_total,
                );
                record_gpu_timing(
                    &stored,
                    &stored_settings,
                    &mut stored_trace,
                    &mut stored_total,
                );
            } else {
                record_gpu_timing(
                    &stored,
                    &stored_settings,
                    &mut stored_trace,
                    &mut stored_total,
                );
                record_gpu_timing(
                    &reference,
                    &settings,
                    &mut reference_trace,
                    &mut reference_total,
                );
            }
        }
        runs.push(serde_json::json!({
            "run": run + 1,
            "reference": {
                "trace": timing_distribution(reference_trace),
                "total_gpu": timing_distribution(reference_total)
            },
            "stored": {
                "trace": timing_distribution(stored_trace),
                "total_gpu": timing_distribution(stored_total)
            }
        }));
    }
    let report = serde_json::json!({
        "probe": "U7 stored inverse cube timing",
        "adapter": reference.ctx.report,
        "settings": settings,
        "warmup_frames_per_variant_per_run": 120,
        "measured_frames_per_variant_per_run": 300,
        "run_count": 5,
        "quality": {
            "primary_status_disagreements": status_disagreements,
            "max_rgb_error": max_color_error
        },
        "runs": runs
    });
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("out/u7");
    std::fs::create_dir_all(&output_dir).unwrap();
    let output_path = output_dir.join("paired-timing-80x64-atlas64-2026-10-03.json");
    std::fs::write(&output_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("U7 paired timing report: {report}");
    println!("U7 timing artifact: out/u7/paired-timing-80x64-atlas64-2026-10-03.json");
}

fn submit_gpu_timing(renderer: &Renderer, settings: &Settings) -> serde_json::Value {
    renderer
        .submit(settings, true)
        .unwrap()
        .map(|ticket| ticket.read(&renderer.ctx).unwrap())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn record_gpu_timing(
    renderer: &Renderer,
    settings: &Settings,
    trace: &mut Vec<f64>,
    total: &mut Vec<f64>,
) {
    record_named_gpu_timing(renderer, settings, "trace-0", trace, total);
}

fn record_named_gpu_timing(
    renderer: &Renderer,
    settings: &Settings,
    pass_name: &str,
    pass_samples: &mut Vec<f64>,
    total: &mut Vec<f64>,
) {
    let timing = submit_gpu_timing(renderer, settings);
    if let Some(pass_ms) = timing.get(pass_name).and_then(serde_json::Value::as_f64) {
        pass_samples.push(pass_ms);
    }
    if let Some(pass_timings) = timing.as_object() {
        let total_ms: f64 = pass_timings
            .values()
            .filter_map(serde_json::Value::as_f64)
            .sum();
        if !pass_timings.is_empty() {
            total.push(total_ms);
        }
    }
}

fn timing_distribution(mut samples: Vec<f64>) -> serde_json::Value {
    samples.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        if samples.is_empty() {
            serde_json::Value::Null
        } else {
            let index = ((samples.len() - 1) as f64 * fraction).round() as usize;
            serde_json::json!(samples[index])
        }
    };
    serde_json::json!({
        "sample_count": samples.len(),
        "median_ms": percentile(0.5),
        "p95_ms": percentile(0.95),
        "p99_ms": percentile(0.99)
    })
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn compute_edgefill_matches_raster_maps_and_reports_pass_time() {
    let settings = Settings {
        mesh: MeshKind::Cube,
        atlas: 64,
        width: 80,
        height: 64,
        texture_set: 3,
        time: 0.0,
        animation_amount: 0.0,
        ..Settings::default()
    };
    let mut raster = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let raster_maps = raster.atlas_planes().unwrap();
    let raster_frame = raster.render_capture(&settings).unwrap();

    let compute_settings = Settings {
        compute_edgefill: true,
        ..settings.clone()
    };
    let mut compute = pollster::block_on(Renderer::headless(
        &compute_settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let compute_maps = compute.atlas_planes().unwrap();
    let compute_frame = compute.render_capture(&compute_settings).unwrap();
    let max_map_error = raster_maps
        .iter()
        .flatten()
        .flatten()
        .zip(compute_maps.iter().flatten().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    let status_disagreements = raster_frame[2]
        .iter()
        .zip(&compute_frame[2])
        .filter(|(a, b)| a[0] != b[0])
        .count();
    let color_error = raster_frame[0]
        .iter()
        .zip(&compute_frame[0])
        .flat_map(|(a, b)| (0..3).map(move |c| (a[c] - b[c]).abs()))
        .fold(0.0f32, f32::max);
    assert!(max_map_error <= 1e-5, "U8 map error was {max_map_error}");
    assert_eq!(status_disagreements, 0);
    assert!(color_error <= 1e-5, "U8 color error was {color_error}");

    let mut raster_times = Vec::new();
    let mut compute_times = Vec::new();
    for _ in 0..12 {
        raster.render_capture(&settings).unwrap();
        compute.render_capture(&compute_settings).unwrap();
        if let Some(ms) = raster
            .last_timings
            .as_ref()
            .and_then(|timings| timings["edgefill-0"].as_f64())
        {
            raster_times.push(ms);
        }
        if let Some(ms) = compute
            .last_timings
            .as_ref()
            .and_then(|timings| timings["edgefill-compute"].as_f64())
        {
            compute_times.push(ms);
        }
    }
    let median = |samples: &mut Vec<f64>| {
        samples.sort_by(f64::total_cmp);
        samples.get(samples.len() / 2).copied()
    };
    println!(
        "U8 raster maps max error={max_map_error}, status disagreements={status_disagreements}, RGB error={color_error}, median edge pass ms raster={:?} compute={:?}",
        median(&mut raster_times),
        median(&mut compute_times)
    );
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn compute_edgefill_bent_tube_seams_match_raster_across_poses() {
    let settings = Settings {
        mesh: MeshKind::TubePinched,
        around: 8,
        long: 16,
        atlas: 128,
        width: 96,
        height: 72,
        texture_set: 0,
        time: 8.0,
        animation_amount: 1.0,
        ..Settings::default()
    };
    let mut raster = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let compute_settings = Settings {
        compute_edgefill: true,
        ..settings.clone()
    };
    let mut compute = pollster::block_on(Renderer::headless(
        &compute_settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let mut max_map_error_all = 0.0f32;
    let mut max_rgb_error_all = 0.0f32;
    let mut max_status_disagreements = 0usize;
    let mut max_new_failures = 0usize;
    for time in [0.0, 4.0, 8.0] {
        let raster_settings = Settings {
            time,
            ..settings.clone()
        };
        let compute_settings = Settings {
            time,
            ..compute_settings.clone()
        };
        let raster_frame = raster.render_capture(&raster_settings).unwrap();
        let compute_frame = compute.render_capture(&compute_settings).unwrap();
        let raster_maps = raster.atlas_planes().unwrap();
        let compute_maps = compute.atlas_planes().unwrap();
        let max_map_error = raster_maps
            .iter()
            .flatten()
            .flatten()
            .zip(compute_maps.iter().flatten().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        let status_disagreements = raster_frame[2]
            .iter()
            .chain(&raster_frame[3])
            .zip(compute_frame[2].iter().chain(&compute_frame[3]))
            .filter(|(a, b)| a[0] != b[0])
            .count();
        let new_failures = raster_frame[2]
            .iter()
            .zip(&compute_frame[2])
            .chain(raster_frame[3].iter().zip(&compute_frame[3]))
            .filter(|(a, b)| (b[0] == 3.0 || b[0] == 4.0) && a[0] != b[0])
            .count();
        let max_rgb_error = raster_frame[0]
            .iter()
            .zip(&compute_frame[0])
            .flat_map(|(a, b)| (0..3).map(move |channel| (a[channel] - b[channel]).abs()))
            .fold(0.0f32, f32::max);
        assert!(
            max_map_error <= 1e-5,
            "pose {time}: map error {max_map_error}"
        );
        assert_eq!(status_disagreements, 0, "pose {time}");
        assert_eq!(new_failures, 0, "pose {time}");
        assert!(
            max_rgb_error <= 1e-5,
            "pose {time}: RGB error {max_rgb_error}"
        );
        max_map_error_all = max_map_error_all.max(max_map_error);
        max_rgb_error_all = max_rgb_error_all.max(max_rgb_error);
        max_status_disagreements = max_status_disagreements.max(status_disagreements);
        max_new_failures = max_new_failures.max(new_failures);
        println!(
            "U8 TubePinched pose={time}: map error={max_map_error}, statuses={status_disagreements}, new failures={new_failures}, RGB error={max_rgb_error}"
        );
    }
    let mut runs = Vec::new();
    for run in 0..5 {
        let mut raster_edgefill = Vec::new();
        let mut raster_total = Vec::new();
        let mut compute_edgefill = Vec::new();
        let mut compute_total = Vec::new();
        if run % 2 == 0 {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&raster, &settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&compute, &compute_settings);
            }
        } else {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&compute, &compute_settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&raster, &settings);
            }
        }
        for sample in 0..300 {
            if (run + sample) % 2 == 0 {
                record_named_gpu_timing(
                    &raster,
                    &settings,
                    "edgefill-0",
                    &mut raster_edgefill,
                    &mut raster_total,
                );
                record_named_gpu_timing(
                    &compute,
                    &compute_settings,
                    "edgefill-compute",
                    &mut compute_edgefill,
                    &mut compute_total,
                );
            } else {
                record_named_gpu_timing(
                    &compute,
                    &compute_settings,
                    "edgefill-compute",
                    &mut compute_edgefill,
                    &mut compute_total,
                );
                record_named_gpu_timing(
                    &raster,
                    &settings,
                    "edgefill-0",
                    &mut raster_edgefill,
                    &mut raster_total,
                );
            }
        }
        runs.push(serde_json::json!({
            "run": run + 1,
            "raster": {
                "edgefill": timing_distribution(raster_edgefill),
                "total_gpu": timing_distribution(raster_total)
            },
            "compute": {
                "edgefill": timing_distribution(compute_edgefill),
                "total_gpu": timing_distribution(compute_total)
            }
        }));
    }
    let report = serde_json::json!({
        "probe": "U8 TubePinched compute edgefill timing",
        "adapter": raster.ctx.report,
        "raster_settings": settings,
        "compute_settings": compute_settings,
        "quality": {
            "max_map_error": max_map_error_all,
            "max_status_disagreements": max_status_disagreements,
            "max_new_budget_or_invalid": max_new_failures,
            "max_rgb_error": max_rgb_error_all
        },
        "warmup_frames_per_variant_per_run": 120,
        "measured_frames_per_variant_per_run": 300,
        "run_count": 5,
        "runs": runs
    });
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("out/u8");
    std::fs::create_dir_all(&output_dir).unwrap();
    let output_path = output_dir.join("tube-pinched-paired-timing-96x72-atlas128-2026-10-03.json");
    std::fs::write(&output_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("U8 TubePinched paired timing report: {report}");
    println!(
        "U8 timing artifact: out/u8/tube-pinched-paired-timing-96x72-atlas128-2026-10-03.json"
    );
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn indirect_edgefill_probe_reports_quality_and_trace_cost() {
    let settings = Settings {
        mesh: MeshKind::Cube,
        atlas: 64,
        width: 80,
        height: 64,
        texture_set: 3,
        time: 0.0,
        animation_amount: 0.0,
        ..Settings::default()
    };
    let mut raster = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let baseline = raster.render_capture(&settings).unwrap();
    let indirect_settings = Settings {
        indirect_edgefill: true,
        ..settings.clone()
    };
    let mut indirect = pollster::block_on(Renderer::headless(
        &indirect_settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let candidate = indirect.render_capture(&indirect_settings).unwrap();
    let pixels = (settings.width * settings.height) as usize;
    let hit_disagreements = baseline[2]
        .iter()
        .zip(&candidate[2])
        .filter(|(a, b)| (a[0] == 1.0) != (b[0] == 1.0))
        .count();
    let status_disagreements = baseline[2]
        .iter()
        .zip(&candidate[2])
        .filter(|(a, b)| a[0] != b[0])
        .count();
    let new_trace_failures = baseline[2]
        .iter()
        .zip(&candidate[2])
        .filter(|(a, b)| (b[0] == 3.0 || b[0] == 4.0) && a[0] != b[0])
        .count();
    let max_rgb_error = baseline[0]
        .iter()
        .zip(&candidate[0])
        .flat_map(|(a, b)| (0..3).map(move |channel| (a[channel] - b[channel]).abs()))
        .fold(0.0f32, f32::max);
    assert!(baseline.iter().flatten().flatten().all(|x| x.is_finite()));
    assert!(candidate.iter().flatten().flatten().all(|x| x.is_finite()));
    let mut runs = Vec::new();
    for run in 0..5 {
        let mut raster_trace = Vec::new();
        let mut raster_total = Vec::new();
        let mut indirect_trace = Vec::new();
        let mut indirect_total = Vec::new();
        if run % 2 == 0 {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&raster, &settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&indirect, &indirect_settings);
            }
        } else {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&indirect, &indirect_settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&raster, &settings);
            }
        }
        for sample in 0..300 {
            if (run + sample) % 2 == 0 {
                record_gpu_timing(&raster, &settings, &mut raster_trace, &mut raster_total);
                record_gpu_timing(
                    &indirect,
                    &indirect_settings,
                    &mut indirect_trace,
                    &mut indirect_total,
                );
            } else {
                record_gpu_timing(
                    &indirect,
                    &indirect_settings,
                    &mut indirect_trace,
                    &mut indirect_total,
                );
                record_gpu_timing(&raster, &settings, &mut raster_trace, &mut raster_total);
            }
        }
        runs.push(serde_json::json!({
            "run": run + 1,
            "raster": {
                "trace": timing_distribution(raster_trace),
                "total_gpu": timing_distribution(raster_total)
            },
            "indirect": {
                "trace": timing_distribution(indirect_trace),
                "total_gpu": timing_distribution(indirect_total)
            }
        }));
    }
    let report = serde_json::json!({
        "probe": "U10 trace-time edgefill indirection cube comparison",
        "adapter": raster.ctx.report,
        "settings": settings,
        "indirect_settings": indirect_settings,
        "quality": {
            "hit_disagreements": hit_disagreements,
            "status_disagreements": status_disagreements,
            "new_budget_or_invalid": new_trace_failures,
            "max_rgb_error": max_rgb_error
        },
        "warmup_frames_per_variant_per_run": 120,
        "measured_frames_per_variant_per_run": 300,
        "run_count": 5,
        "runs": runs
    });
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("out/u10");
    std::fs::create_dir_all(&output_dir).unwrap();
    let output_path = output_dir.join("indirect-edgefill-cube-80x64-atlas64-2026-10-03.json");
    std::fs::write(&output_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!(
        "U10 hit-mask delta={hit_disagreements}/{pixels} ({:.4}%), status disagreements={status_disagreements}, new budget/invalid={new_trace_failures}, max RGB error={max_rgb_error}",
        hit_disagreements as f64 * 100.0 / pixels as f64
    );
    println!("U10 paired timing report: {report}");
    println!("U10 timing artifact: out/u10/indirect-edgefill-cube-80x64-atlas64-2026-10-03.json");
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn split_teleport_maps_preserve_seam_data_and_report_trace_cost() {
    let settings = Settings {
        mesh: MeshKind::Cube,
        atlas: 64,
        width: 80,
        height: 64,
        texture_set: 3,
        time: 0.0,
        animation_amount: 0.0,
        ..Settings::default()
    };
    let mut packed = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let packed_map = packed.ctx.read_float(&packed.teleport.texture).unwrap();
    let packed_frame = packed.render_capture(&settings).unwrap();

    let split_settings = Settings {
        split_teleport: true,
        ..settings.clone()
    };
    let mut split = pollster::block_on(Renderer::headless(
        &split_settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let distance_bytes = split.ctx.read_texture(&split.teleport.texture, 4).unwrap();
    let destination = split.teleport_destination.as_ref().unwrap();
    let destination_bytes = split.ctx.read_texture(&destination.texture, 8).unwrap();
    let mut max_map_error = 0.0f32;
    for (index, packed_pixel) in packed_map.iter().enumerate() {
        let distance =
            f32::from_le_bytes(distance_bytes[index * 4..index * 4 + 4].try_into().unwrap());
        let destination_u = f32::from_le_bytes(
            destination_bytes[index * 8..index * 8 + 4]
                .try_into()
                .unwrap(),
        );
        let destination_v = f32::from_le_bytes(
            destination_bytes[index * 8 + 4..index * 8 + 8]
                .try_into()
                .unwrap(),
        );
        max_map_error = max_map_error
            .max((distance - packed_pixel[2]).abs())
            .max((destination_u - packed_pixel[0]).abs())
            .max((destination_v - packed_pixel[1]).abs());
    }
    let split_frame = split.render_capture(&split_settings).unwrap();
    let primary_status_disagreements = packed_frame[2]
        .iter()
        .zip(&split_frame[2])
        .filter(|(a, b)| a[0] != b[0])
        .count();
    let shadow_status_disagreements = packed_frame[3]
        .iter()
        .zip(&split_frame[3])
        .filter(|(a, b)| a[0] != b[0])
        .count();
    let max_rgb_error = packed_frame[0]
        .iter()
        .zip(&split_frame[0])
        .flat_map(|(a, b)| (0..3).map(move |channel| (a[channel] - b[channel]).abs()))
        .fold(0.0f32, f32::max);
    assert!(max_map_error <= 1e-5, "U12 map error was {max_map_error}");
    assert_eq!(primary_status_disagreements, 0);
    assert_eq!(shadow_status_disagreements, 0);
    assert!(max_rgb_error <= 1e-5, "U12 RGB error was {max_rgb_error}");

    let packed_debug_settings = Settings {
        debug_max_steps: 1,
        ..settings.clone()
    };
    let split_debug_settings = Settings {
        debug_max_steps: 1,
        ..split_settings.clone()
    };
    let packed_debug_frame = packed.render_capture(&packed_debug_settings).unwrap();
    let split_debug_frame = split.render_capture(&split_debug_settings).unwrap();
    assert!(
        packed_debug_frame[2].iter().any(|trace| trace[0] == 5.0),
        "forced-hit diagnostic did not launch primary rays"
    );
    assert_eq!(
        packed_debug_frame[2]
            .iter()
            .zip(&split_debug_frame[2])
            .filter(|(a, b)| a[0] != b[0])
            .count(),
        0,
        "split teleport mode must remain active with forced-hit diagnostics"
    );

    let mut runs = Vec::new();
    for run in 0..5 {
        let mut packed_trace = Vec::new();
        let mut packed_total = Vec::new();
        let mut split_trace = Vec::new();
        let mut split_total = Vec::new();
        if run % 2 == 0 {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&packed, &settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&split, &split_settings);
            }
        } else {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&split, &split_settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&packed, &settings);
            }
        }
        for sample in 0..300 {
            if (run + sample) % 2 == 0 {
                record_gpu_timing(&packed, &settings, &mut packed_trace, &mut packed_total);
                record_gpu_timing(&split, &split_settings, &mut split_trace, &mut split_total);
            } else {
                record_gpu_timing(&split, &split_settings, &mut split_trace, &mut split_total);
                record_gpu_timing(&packed, &settings, &mut packed_trace, &mut packed_total);
            }
        }
        runs.push(serde_json::json!({
            "run": run + 1,
            "packed": {
                "trace": timing_distribution(packed_trace),
                "total_gpu": timing_distribution(packed_total)
            },
            "split": {
                "trace": timing_distribution(split_trace),
                "total_gpu": timing_distribution(split_total)
            }
        }));
    }
    let report = serde_json::json!({
        "probe": "U12 split teleport map cube comparison",
        "adapter": packed.ctx.report,
        "packed_settings": settings,
        "split_settings": split_settings,
        "quality": {
            "max_map_error": max_map_error,
            "primary_status_disagreements": primary_status_disagreements,
            "shadow_status_disagreements": shadow_status_disagreements,
            "max_rgb_error": max_rgb_error,
            "bytes_per_texel_packed": 16,
            "bytes_per_texel_split": 12
        },
        "warmup_frames_per_variant_per_run": 120,
        "measured_frames_per_variant_per_run": 300,
        "run_count": 5,
        "runs": runs
    });
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("out/u12");
    std::fs::create_dir_all(&output_dir).unwrap();
    let output_path = output_dir.join("split-teleport-cube-80x64-atlas64-2026-10-03.json");
    std::fs::write(&output_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("U12 paired timing report: {report}");
    println!("U12 timing artifact: out/u12/split-teleport-cube-80x64-atlas64-2026-10-03.json");
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn compact_warp_probe_reports_quality_and_trace_cost() {
    let settings = Settings {
        mesh: MeshKind::Cube,
        atlas: 64,
        width: 80,
        height: 64,
        texture_set: 3,
        time: 0.0,
        animation_amount: 0.0,
        ..Settings::default()
    };
    let mut full = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let full_maps = full.atlas_planes().unwrap();
    let full_frame = full.render_capture(&settings).unwrap();

    let compact_settings = Settings {
        compact_warp: true,
        ..settings.clone()
    };
    let mut compact = pollster::block_on(Renderer::headless(
        &compact_settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let compact_maps = compact.atlas_planes().unwrap();
    let compact_frame = compact.render_capture(&compact_settings).unwrap();
    let max_map_error = full_maps
        .iter()
        .flatten()
        .flatten()
        .zip(compact_maps.iter().flatten().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    let primary_status_disagreements = full_frame[2]
        .iter()
        .zip(&compact_frame[2])
        .filter(|(a, b)| a[0] != b[0])
        .count();
    let shadow_status_disagreements = full_frame[3]
        .iter()
        .zip(&compact_frame[3])
        .filter(|(a, b)| a[0] != b[0])
        .count();
    let max_rgb_error = full_frame[0]
        .iter()
        .zip(&compact_frame[0])
        .flat_map(|(a, b)| (0..3).map(move |channel| (a[channel] - b[channel]).abs()))
        .fold(0.0f32, f32::max);
    assert!(
        compact_maps
            .iter()
            .flatten()
            .flatten()
            .all(|x| x.is_finite())
    );

    let mut full_times = Vec::new();
    let mut compact_times = Vec::new();
    for sample in 0..14 {
        full.render_capture(&settings).unwrap();
        compact.render_capture(&compact_settings).unwrap();
        if sample >= 2 {
            if let Some(ms) = full
                .last_timings
                .as_ref()
                .and_then(|timings| timings["trace-0"].as_f64())
            {
                full_times.push(ms);
            }
            if let Some(ms) = compact
                .last_timings
                .as_ref()
                .and_then(|timings| timings["trace-0"].as_f64())
            {
                compact_times.push(ms);
            }
        }
    }
    let median = |samples: &mut Vec<f64>| {
        samples.sort_by(f64::total_cmp);
        samples.get(samples.len() / 2).copied()
    };
    println!(
        "U14 max transform-map error={max_map_error}, primary/shadow status disagreements={primary_status_disagreements}/{shadow_status_disagreements}, max RGB error={max_rgb_error}, transform storage bytes/texel=128/64 (raw+warp), median trace ms full={:?} RGBA16F={:?}",
        median(&mut full_times),
        median(&mut compact_times)
    );
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn compact_warp_probe_reports_pinched_bent_quality() {
    let settings = Settings {
        mesh: MeshKind::TubePinched,
        around: 8,
        long: 16,
        atlas: 128,
        width: 120,
        height: 90,
        texture_set: 0,
        time: 8.0,
        animation_amount: 1.0,
        ..Settings::default()
    };
    let mut full = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let full_frame = full.render_capture(&settings).unwrap();
    let full_maps = full.atlas_planes().unwrap();
    let compact_settings = Settings {
        compact_warp: true,
        ..settings.clone()
    };
    let mut compact = pollster::block_on(Renderer::headless(
        &compact_settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let compact_frame = compact.render_capture(&compact_settings).unwrap();
    let compact_maps = compact.atlas_planes().unwrap();
    let max_map_error = full_maps
        .iter()
        .flatten()
        .flatten()
        .zip(compact_maps.iter().flatten().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    let primary_status_delta = full_frame[2]
        .iter()
        .zip(&compact_frame[2])
        .filter(|(a, b)| a[0] != b[0])
        .count();
    let shadow_status_delta = full_frame[3]
        .iter()
        .zip(&compact_frame[3])
        .filter(|(a, b)| a[0] != b[0])
        .count();
    let new_failures = full_frame[2]
        .iter()
        .zip(&compact_frame[2])
        .chain(full_frame[3].iter().zip(&compact_frame[3]))
        .filter(|(a, b)| (b[0] == 3.0 || b[0] == 4.0) && a[0] != b[0])
        .count();
    let max_rgb_error = full_frame[0]
        .iter()
        .zip(&compact_frame[0])
        .flat_map(|(a, b)| (0..3).map(move |channel| (a[channel] - b[channel]).abs()))
        .fold(0.0f32, f32::max);
    assert!(
        compact_maps
            .iter()
            .flatten()
            .flatten()
            .all(|value| value.is_finite())
    );
    println!(
        "U14 bent TubePinched: map error={max_map_error}, primary/shadow status delta={primary_status_delta}/{shadow_status_delta}, new budget/invalid={new_failures}, max RGB error={max_rgb_error}"
    );
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn specialized_inverse_repeated_probe_measures_cube_output_and_trace_time() {
    let settings = Settings {
        mesh: MeshKind::Cube,
        atlas: 64,
        width: 80,
        height: 64,
        texture_set: 3,
        time: 0.0,
        animation_amount: 0.0,
        ..Settings::default()
    };
    let mut reference = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let reference_frame = reference.render_capture(&settings).unwrap();
    let optimized_settings = Settings {
        specialized_inverse: true,
        ..settings.clone()
    };
    let mut optimized = pollster::block_on(Renderer::headless(
        &optimized_settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let optimized_frame = optimized.render_capture(&optimized_settings).unwrap();
    let status_disagreements = reference_frame[2]
        .iter()
        .chain(&reference_frame[3])
        .zip(optimized_frame[2].iter().chain(&optimized_frame[3]))
        .filter(|(a, b)| a[0] != b[0])
        .count();
    let max_rgb_error = reference_frame[0]
        .iter()
        .zip(&optimized_frame[0])
        .flat_map(|(a, b)| (0..3).map(move |channel| (a[channel] - b[channel]).abs()))
        .fold(0.0f32, f32::max);
    assert_eq!(status_disagreements, 0);
    assert!(max_rgb_error <= 1e-5);

    let mut runs = Vec::new();
    for run in 0..5 {
        let mut reference_trace = Vec::new();
        let mut reference_total = Vec::new();
        let mut optimized_trace = Vec::new();
        let mut optimized_total = Vec::new();
        if run % 2 == 0 {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&reference, &settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&optimized, &optimized_settings);
            }
        } else {
            for _ in 0..120 {
                let _ = submit_gpu_timing(&optimized, &optimized_settings);
            }
            for _ in 0..120 {
                let _ = submit_gpu_timing(&reference, &settings);
            }
        }
        for sample in 0..300 {
            if (run + sample) % 2 == 0 {
                record_gpu_timing(
                    &reference,
                    &settings,
                    &mut reference_trace,
                    &mut reference_total,
                );
                record_gpu_timing(
                    &optimized,
                    &optimized_settings,
                    &mut optimized_trace,
                    &mut optimized_total,
                );
            } else {
                record_gpu_timing(
                    &optimized,
                    &optimized_settings,
                    &mut optimized_trace,
                    &mut optimized_total,
                );
                record_gpu_timing(
                    &reference,
                    &settings,
                    &mut reference_trace,
                    &mut reference_total,
                );
            }
        }
        runs.push(serde_json::json!({
            "run": run + 1,
            "general": {
                "trace": timing_distribution(reference_trace),
                "total_gpu": timing_distribution(reference_total)
            },
            "cofactor": {
                "trace": timing_distribution(optimized_trace),
                "total_gpu": timing_distribution(optimized_total)
            }
        }));
    }
    let report = serde_json::json!({
        "probe": "U15 specialized inverse cube comparison",
        "adapter": reference.ctx.report,
        "settings": settings,
        "optimized_settings": optimized_settings,
        "quality": {
            "status_disagreements": status_disagreements,
            "max_rgb_error": max_rgb_error
        },
        "warmup_frames_per_variant_per_run": 120,
        "measured_frames_per_variant_per_run": 300,
        "run_count": 5,
        "runs": runs
    });
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("out/u15");
    std::fs::create_dir_all(&output_dir).unwrap();
    let output_path = output_dir.join("specialized-inverse-cube-80x64-atlas64-2026-10-03.json");
    std::fs::write(&output_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("U15 paired timing report: {report}");
    println!("U15 timing artifact: out/u15/specialized-inverse-cube-80x64-atlas64-2026-10-03.json");
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn scale_derived_tolerances_probe_covers_size_and_thickness_buckets() {
    let buckets = [
        ("small", 2.0, 0.5, 0.25),
        ("reference", 4.0, 1.0, 1.0),
        ("large", 8.0, 2.0, 2.0),
        ("thin", 4.0, 1.0, 0.15),
        ("thick", 4.0, 1.0, 6.0),
    ];
    for (name, length, radius, thickness) in buckets {
        let settings = Settings {
            mesh: MeshKind::Tube,
            atlas: 64,
            width: 80,
            height: 64,
            texture_set: 3,
            length,
            radius,
            thickness,
            time: 4.0,
            animation_amount: 1.0,
            ..Settings::default()
        };
        let mut reference = pollster::block_on(Renderer::headless(
            &settings,
            Path::new("demo/assets"),
            false,
            0,
        ))
        .unwrap();
        let reference_frame = reference.render_capture(&settings).unwrap();
        let derived_settings = Settings {
            epsilon_policy: EpsilonPolicy::ScaleDerived,
            ..settings.clone()
        };
        let mut derived = pollster::block_on(Renderer::headless(
            &derived_settings,
            Path::new("demo/assets"),
            false,
            0,
        ))
        .unwrap();
        let derived_frame = derived.render_capture(&derived_settings).unwrap();
        assert!(
            reference_frame
                .iter()
                .flatten()
                .flatten()
                .all(|v| v.is_finite())
        );
        assert!(
            derived_frame
                .iter()
                .flatten()
                .flatten()
                .all(|v| v.is_finite())
        );
        let primary_status_delta = reference_frame[2]
            .iter()
            .zip(&derived_frame[2])
            .filter(|(a, b)| a[0] != b[0])
            .count();
        let shadow_status_delta = reference_frame[3]
            .iter()
            .zip(&derived_frame[3])
            .filter(|(a, b)| a[0] != b[0])
            .count();
        let primary_hit_delta = reference_frame[2]
            .iter()
            .zip(&derived_frame[2])
            .filter(|(a, b)| (a[0] == 1.0) != (b[0] == 1.0))
            .count();
        let shadow_hit_delta = reference_frame[3]
            .iter()
            .zip(&derived_frame[3])
            .filter(|(a, b)| (a[0] == 1.0) != (b[0] == 1.0))
            .count();
        let new_failures = reference_frame[2]
            .iter()
            .zip(&derived_frame[2])
            .chain(reference_frame[3].iter().zip(&derived_frame[3]))
            .filter(|(a, b)| (b[0] == 3.0 || b[0] == 4.0) && a[0] != b[0])
            .count();
        let max_rgb_error = reference_frame[0]
            .iter()
            .zip(&derived_frame[0])
            .flat_map(|(a, b)| (0..3).map(move |channel| (a[channel] - b[channel]).abs()))
            .fold(0.0f32, f32::max);
        println!(
            "U6 {name} length/radius/thickness={length}/{radius}/{thickness}, tolerances={:?}, primary/shadow status delta={primary_status_delta}/{shadow_status_delta}, hit delta={primary_hit_delta}/{shadow_hit_delta}, new budget/invalid={new_failures}, max RGB error={max_rgb_error}",
            derived_settings.resolved_tolerances()
        );
    }
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn traversal_options_repeated_probe_covers_bent_tube_poses() {
    let settings = Settings {
        mesh: MeshKind::TubePinched,
        around: 8,
        long: 16,
        atlas: 128,
        width: 160,
        height: 120,
        texture_set: 0,
        time: 8.0,
        animation_amount: 1.0,
        ..Settings::default()
    };
    let mut reference = pollster::block_on(Renderer::headless(
        &settings,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    let options = Settings {
        hit_refinement: true,
        seam_aware_stepping: true,
        adaptive_steps: true,
        ..settings.clone()
    };
    let mut candidate = pollster::block_on(Renderer::headless(
        &options,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    for pose in [0.0, 2.5, 5.0, 8.0] {
        let reference_pose = Settings {
            time: pose,
            ..settings.clone()
        };
        let candidate_pose = Settings {
            time: pose,
            ..options.clone()
        };
        let a = reference.render_capture(&reference_pose).unwrap();
        let b = candidate.render_capture(&candidate_pose).unwrap();
        let primary_status_delta = a[2].iter().zip(&b[2]).filter(|(x, y)| x[0] != y[0]).count();
        let shadow_status_delta = a[3].iter().zip(&b[3]).filter(|(x, y)| x[0] != y[0]).count();
        let primary_hit_delta = a[2]
            .iter()
            .zip(&b[2])
            .filter(|(x, y)| (x[0] == 1.0) != (y[0] == 1.0))
            .count();
        let shadow_hit_delta = a[3]
            .iter()
            .zip(&b[3])
            .filter(|(x, y)| (x[0] == 1.0) != (y[0] == 1.0))
            .count();
        let new_failures = a[2]
            .iter()
            .zip(&b[2])
            .chain(a[3].iter().zip(&b[3]))
            .filter(|(x, y)| (y[0] == 3.0 || y[0] == 4.0) && x[0] != y[0])
            .count();
        let max_rgb_error = a[0]
            .iter()
            .zip(&b[0])
            .flat_map(|(x, y)| (0..3).map(move |channel| (x[channel] - y[channel]).abs()))
            .fold(0.0f32, f32::max);
        println!(
            "U2-U4 pose={pose} primary/shadow status delta={primary_status_delta}/{shadow_status_delta}, hit delta={primary_hit_delta}/{shadow_hit_delta}, new budget/invalid={new_failures}, max RGB error={max_rgb_error}"
        );
    }
    let final_reference = Settings {
        time: 8.0,
        ..settings.clone()
    };
    let final_candidate = Settings {
        time: 8.0,
        ..options
    };
    let mut reference_times = Vec::new();
    let mut candidate_times = Vec::new();
    for sample in 0..14 {
        reference.render_capture(&final_reference).unwrap();
        candidate.render_capture(&final_candidate).unwrap();
        if sample >= 2 {
            if let Some(ms) = reference
                .last_timings
                .as_ref()
                .and_then(|timings| timings["trace-0"].as_f64())
            {
                reference_times.push(ms);
            }
            if let Some(ms) = candidate
                .last_timings
                .as_ref()
                .and_then(|timings| timings["trace-0"].as_f64())
            {
                candidate_times.push(ms);
            }
        }
    }
    let median = |samples: &mut Vec<f64>| {
        samples.sort_by(f64::total_cmp);
        samples.get(samples.len() / 2).copied()
    };
    println!(
        "U2-U4 time=8.0 paired median trace ms reference={:?} options={:?}",
        median(&mut reference_times),
        median(&mut candidate_times)
    );
}

#[test]
#[ignore = "requires a real native GPU; run mise run gpu-test"]
fn traversal_options_individual_probe_covers_bent_tube_poses() {
    let base = Settings {
        mesh: MeshKind::TubePinched,
        around: 8,
        long: 16,
        atlas: 128,
        width: 120,
        height: 90,
        texture_set: 0,
        time: 8.0,
        animation_amount: 1.0,
        ..Settings::default()
    };
    let candidates = vec![
        (
            "U2",
            Settings {
                hit_refinement: true,
                ..base.clone()
            },
        ),
        (
            "U3",
            Settings {
                seam_aware_stepping: true,
                ..base.clone()
            },
        ),
        (
            "U4",
            Settings {
                adaptive_steps: true,
                ..base.clone()
            },
        ),
    ];
    let mut renderer = pollster::block_on(Renderer::headless(
        &base,
        Path::new("demo/assets"),
        false,
        0,
    ))
    .unwrap();
    for pose in [0.0, 2.5, 5.0, 8.0] {
        let reference_settings = Settings {
            time: pose,
            ..base.clone()
        };
        let reference_frame = renderer.render_capture(&reference_settings).unwrap();
        let reference_ms = renderer
            .last_timings
            .as_ref()
            .and_then(|timings| timings["trace-0"].as_f64());
        for (name, candidate_settings) in &candidates {
            let candidate_pose = Settings {
                time: pose,
                ..candidate_settings.clone()
            };
            let frame = renderer.render_capture(&candidate_pose).unwrap();
            let primary_status_delta = reference_frame[2]
                .iter()
                .zip(&frame[2])
                .filter(|(a, b)| a[0] != b[0])
                .count();
            let shadow_status_delta = reference_frame[3]
                .iter()
                .zip(&frame[3])
                .filter(|(a, b)| a[0] != b[0])
                .count();
            let primary_hit_delta = reference_frame[2]
                .iter()
                .zip(&frame[2])
                .filter(|(a, b)| (a[0] == 1.0) != (b[0] == 1.0))
                .count();
            let shadow_hit_delta = reference_frame[3]
                .iter()
                .zip(&frame[3])
                .filter(|(a, b)| (a[0] == 1.0) != (b[0] == 1.0))
                .count();
            let new_failures = reference_frame[2]
                .iter()
                .zip(&frame[2])
                .chain(reference_frame[3].iter().zip(&frame[3]))
                .filter(|(a, b)| (b[0] == 3.0 || b[0] == 4.0) && a[0] != b[0])
                .count();
            let max_rgb_error = reference_frame[0]
                .iter()
                .zip(&frame[0])
                .flat_map(|(a, b)| (0..3).map(move |channel| (a[channel] - b[channel]).abs()))
                .fold(0.0f32, f32::max);
            let trace_ms = renderer
                .last_timings
                .as_ref()
                .and_then(|timings| timings["trace-0"].as_f64());
            println!(
                "{name} pose={pose}: primary/shadow status delta={primary_status_delta}/{shadow_status_delta}, hit delta={primary_hit_delta}/{shadow_hit_delta}, new budget/invalid={new_failures}, max RGB error={max_rgb_error}, trace ms reference/candidate={reference_ms:?}/{trace_ms:?}"
            );
        }
    }
    let final_reference = Settings {
        time: 8.0,
        ..base.clone()
    };
    let mut reference_times = Vec::new();
    let mut option_times = [Vec::new(), Vec::new(), Vec::new()];
    for sample in 0..14 {
        renderer.render_capture(&final_reference).unwrap();
        if sample >= 2
            && let Some(ms) = renderer
                .last_timings
                .as_ref()
                .and_then(|timings| timings["trace-0"].as_f64())
        {
            reference_times.push(ms);
        }
        for (index, (_, candidate_settings)) in candidates.iter().enumerate() {
            renderer.render_capture(candidate_settings).unwrap();
            if sample >= 2
                && let Some(ms) = renderer
                    .last_timings
                    .as_ref()
                    .and_then(|timings| timings["trace-0"].as_f64())
            {
                option_times[index].push(ms);
            }
        }
    }
    let median = |samples: &mut Vec<f64>| {
        samples.sort_by(f64::total_cmp);
        samples.get(samples.len() / 2).copied()
    };
    let full_reference_ms = median(&mut reference_times).unwrap();
    let full_option_ms = [
        median(&mut option_times[0]).unwrap(),
        median(&mut option_times[1]).unwrap(),
        median(&mut option_times[2]).unwrap(),
    ];
    println!(
        "U2-U4 lighting+shadow median trace ms reference/U2/U3/U4={full_reference_ms:?}/{:?}/{:?}/{:?}",
        full_option_ms[0], full_option_ms[1], full_option_ms[2]
    );

    let primary_reference = Settings {
        time: 8.0,
        lighting_mode: 0,
        ..base.clone()
    };
    let mut primary_reference_times = Vec::new();
    let mut primary_option_times = [Vec::new(), Vec::new(), Vec::new()];
    for sample in 0..14 {
        renderer.render_capture(&primary_reference).unwrap();
        if sample >= 2
            && let Some(ms) = renderer
                .last_timings
                .as_ref()
                .and_then(|timings| timings["trace-0"].as_f64())
        {
            primary_reference_times.push(ms);
        }
        for (index, (_, candidate_settings)) in candidates.iter().enumerate() {
            let primary_candidate = Settings {
                time: 8.0,
                lighting_mode: 0,
                ..candidate_settings.clone()
            };
            renderer.render_capture(&primary_candidate).unwrap();
            if sample >= 2
                && let Some(ms) = renderer
                    .last_timings
                    .as_ref()
                    .and_then(|timings| timings["trace-0"].as_f64())
            {
                primary_option_times[index].push(ms);
            }
        }
    }
    let primary_reference_ms = median(&mut primary_reference_times).unwrap();
    let primary_option_ms = [
        median(&mut primary_option_times[0]).unwrap(),
        median(&mut primary_option_times[1]).unwrap(),
        median(&mut primary_option_times[2]).unwrap(),
    ];
    println!(
        "U2-U4 primary-only median trace ms reference/U2/U3/U4={primary_reference_ms:?}/{:?}/{:?}/{:?}; shadow-enabled shading delta ms={:?}/{:?}/{:?}/{:?}",
        primary_option_ms[0],
        primary_option_ms[1],
        primary_option_ms[2],
        full_reference_ms - primary_reference_ms,
        full_option_ms[0] - primary_option_ms[0],
        full_option_ms[1] - primary_option_ms[1],
        full_option_ms[2] - primary_option_ms[2]
    );
}
