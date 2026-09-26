//! JSON round-trip property tests (P1-1): for every spec type, arbitrary
//! values serialise → deserialise → serialise to the identical string.
//! Floats cover the full finite range, including subnormals and signed zero,
//! so serde_json's shortest round-trip formatting is exercised.

use std::collections::BTreeMap;

use etendue_scene::{
    BakedRobot, BakedSample, BakedScenario, CameraSpec, CaptureEvent, FrameRef, FrameSpec,
    LaserSpec, LightShape, LightSpec, ManifestJoint, ManifestLicense, ManifestSource,
    ManifestVisual, PartSpec, RigSpec, RobotManifest, RobotSpec, ScenarioSpec, SceneSpec, Step,
    TargetGeometry, TargetSpec,
};
use nalgebra::{Isometry3, Quaternion, Translation3, UnitQuaternion};
use proptest::collection::vec;
use proptest::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use vision_calibration_core::{
    BrownConrady5, CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams,
    ProjectionParams, ScheimpflugParams, SensorParams,
};

fn roundtrip<T: Serialize + DeserializeOwned>(value: &T) {
    let first = serde_json::to_string(value).expect("serialise");
    let back: T = serde_json::from_str(&first).expect("deserialise");
    let second = serde_json::to_string(&back).expect("re-serialise");
    assert_eq!(first, second);
}

fn num() -> impl Strategy<Value = f64> {
    use proptest::num::f64::*;
    POSITIVE | NEGATIVE | NORMAL | SUBNORMAL | ZERO
}

fn small() -> impl Strategy<Value = f64> {
    -10.0..10.0f64
}

fn ident() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_-]{0,7}"
}

fn frame_ref() -> impl Strategy<Value = FrameRef> {
    prop_oneof![
        Just(FrameRef::world()),
        ident().prop_map(|id| FrameRef::entity(&id)),
        (ident(), "[a-z][a-z0-9_]{0,7}").prop_map(|(r, l)| FrameRef::robot_link(&r, &l)),
    ]
}

fn pose() -> impl Strategy<Value = Isometry3<f64>> {
    (
        (-1.0..1.0f64, -1.0..1.0f64, -1.0..1.0f64, 0.1..1.0f64),
        (small(), small(), small()),
    )
        .prop_map(|((x, y, z, w), (tx, ty, tz))| {
            Isometry3::from_parts(
                Translation3::new(tx, ty, tz),
                UnitQuaternion::from_quaternion(Quaternion::new(w, x, y, z)),
            )
        })
}

fn camera_params() -> impl Strategy<Value = CameraParams> {
    let distortion = prop_oneof![
        Just(DistortionParams::None),
        (num(), num(), num(), num(), num(), 0u32..20).prop_map(|(k1, k2, k3, p1, p2, iters)| {
            DistortionParams::BrownConrady5 {
                params: BrownConrady5 {
                    k1,
                    k2,
                    k3,
                    p1,
                    p2,
                    iters,
                },
            }
        }),
        num().prop_map(|lambda| DistortionParams::Division { lambda }),
    ];
    let sensor = prop_oneof![
        Just(SensorParams::Identity),
        (num(), num()).prop_map(|(tilt_x, tilt_y)| SensorParams::Scheimpflug {
            params: ScheimpflugParams { tilt_x, tilt_y },
        }),
        [
            num(),
            num(),
            num(),
            num(),
            num(),
            num(),
            num(),
            num(),
            num()
        ]
        .prop_map(|h| {
            SensorParams::Homography {
                h: [[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], h[8]]],
            }
        }),
    ];
    (distortion, sensor, [num(), num(), num(), num(), num()]).prop_map(|(distortion, sensor, k)| {
        CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion,
            sensor,
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx: k[0],
                    fy: k[1],
                    cx: k[2],
                    cy: k[3],
                    skew: k[4],
                },
            },
        }
    })
}

fn board() -> impl Strategy<Value = vision_calibration_dataset::TargetSpec> {
    use vision_calibration_dataset::TargetSpec as Board;
    prop_oneof![
        (1u32..40, 1u32..40, num()).prop_map(|(rows, cols, square_size_m)| Board::Chessboard {
            rows,
            cols,
            square_size_m
        }),
        (
            1u32..40,
            1u32..40,
            num(),
            num(),
            "DICT_[0-9]X[0-9]_[0-9]{2,3}"
        )
            .prop_map(|(rows, cols, square_size_m, marker_size_m, dictionary)| {
                Board::Charuco {
                    rows,
                    cols,
                    square_size_m,
                    marker_size_m,
                    dictionary,
                }
            }),
    ]
}

prop_compose! {
    fn scene()(
        frames in vec((ident(), frame_ref(), pose()), 0..3),
        robots in vec((ident(), frame_ref(), pose(), ".{0,12}", proptest::option::of(vec(num(), 0..7))), 0..3),
        rigs in vec((ident(), frame_ref(), pose()), 0..3),
        cameras in vec((ident(), frame_ref(), pose(), camera_params(), [1u32..10_000, 1u32..10_000]), 0..3),
        lasers in vec((ident(), frame_ref(), pose(), [num(), num(), num(), num()]), 0..3),
        lights in vec((ident(), frame_ref(), pose(), num(), [num(), num(), num()], 0..3u8, [num(), num()]), 0..3),
        targets in vec((ident(), frame_ref(), pose(), prop_oneof![
            board().prop_map(|board| TargetGeometry::Board { board }),
            (num(), num()).prop_map(|(width, height)| TargetGeometry::Rectangle { width, height }),
        ]), 0..3),
        parts in vec((ident(), frame_ref(), pose(), ".{0,12}"), 0..3),
        description in proptest::option::of(".{0,20}"),
    ) -> SceneSpec {
        SceneSpec {
            version: 1,
            frames: frames.into_iter().map(|(id, parent, parent_se3_self)| FrameSpec { id, parent, parent_se3_self }).collect(),
            robots: robots.into_iter().map(|(id, parent, parent_se3_self, manifest, initial_q)| RobotSpec { id, parent, parent_se3_self, manifest, initial_q }).collect(),
            rigs: rigs.into_iter().map(|(id, parent, parent_se3_self)| RigSpec { id, parent, parent_se3_self }).collect(),
            cameras: cameras.into_iter().map(|(id, parent, parent_se3_self, params, resolution)| CameraSpec { id, parent, parent_se3_self, params, resolution }).collect(),
            lasers: lasers.into_iter().map(|(id, parent, parent_se3_self, p)| LaserSpec {
                id, parent, parent_se3_self,
                fan_half_angle: p[0], fan_length: p[1], wavelength_nm: p[2], beam_waist_m: p[3],
            }).collect(),
            lights: lights.into_iter().map(|(id, parent, parent_se3_self, power_w, color, shape, s)| LightSpec {
                id, parent, parent_se3_self, power_w, color,
                shape: match shape {
                    0 => LightShape::Point { radius_m: s[0] },
                    1 => LightShape::Spot { cone_angle: s[0], blend: s[1] },
                    _ => LightShape::Area { size_m: s },
                },
            }).collect(),
            targets: targets.into_iter().map(|(id, parent, parent_se3_self, geometry)| TargetSpec { id, parent, parent_se3_self, geometry }).collect(),
            parts: parts.into_iter().map(|(id, parent, parent_se3_self, mesh)| PartSpec { id, parent, parent_se3_self, mesh }).collect(),
            description,
        }
    }
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (ident(), vec(num(), 0..7), num()).prop_map(|(robot, q, speed_scale)| Step::PtpJoints {
            robot,
            q,
            speed_scale
        }),
        (ident(), pose(), num()).prop_map(|(robot, base_se3_tool, speed_scale)| Step::PtpPose {
            robot,
            base_se3_tool,
            speed_scale
        }),
        (ident(), pose(), num()).prop_map(|(robot, base_se3_tool, speed_scale)| Step::Lin {
            robot,
            base_se3_tool,
            speed_scale
        }),
        proptest::option::of(ident()).prop_map(|id| Step::Capture { id }),
        num().prop_map(|duration_s| Step::Wait { duration_s }),
    ]
}

prop_compose! {
    fn scenario()(dt in num(), steps in vec(step(), 0..8), description in proptest::option::of(".{0,20}")) -> ScenarioSpec {
        ScenarioSpec { version: 1, dt, steps, description }
    }
}

prop_compose! {
    fn baked()(
        dt in num(),
        frames in vec(".{1,10}", 1..5),
        robots in vec((ident(), vec(".{1,8}", 0..4)), 0..3),
        samples in vec((num(), vec(pose(), 0..5), vec(vec(num(), 0..4), 0..3), proptest::option::of(ident())), 0..6),
    ) -> BakedScenario {
        BakedScenario {
            version: 1,
            dt,
            frames,
            robots: robots.into_iter().map(|(id, joint_names)| BakedRobot { id, joint_names }).collect(),
            samples: samples.into_iter().map(|(t, world_se3_frame, joint_positions, capture)| BakedSample {
                t, world_se3_frame, joint_positions, capture: capture.map(|id| CaptureEvent { id }),
            }).collect(),
        }
    }
}

prop_compose! {
    fn manifest()(
        id in ident(),
        joints in vec((".{1,10}", num(), num(), num(), num(), ".{0,20}"), 0..7),
        visuals in vec((".{1,10}", ".{1,20}"), 0..5),
        xacro_args in proptest::collection::btree_map(".{1,8}", ".{0,8}", 0..3),
        redistributable in any::<bool>(),
    ) -> RobotManifest {
        RobotManifest {
            version: 1,
            id,
            name: "Robot".into(),
            urdf: "robot.urdf".into(),
            base_link: "base_link".into(),
            tcp_link: "tool0".into(),
            joints: joints.into_iter().map(|(name, lower, upper, max_velocity, max_acceleration, limit_source)| ManifestJoint {
                name, lower, upper, max_velocity, max_acceleration, limit_source,
            }).collect(),
            visuals: visuals.into_iter().map(|(link, mesh)| ManifestVisual { link, mesh }).collect(),
            source: ManifestSource {
                repository: "https://example.invalid/repo".into(),
                revision: "0123456789abcdef0123456789abcdef01234567".into(),
                entry: "urdf/robot.urdf.xacro".into(),
                xacro_args: xacro_args.into_iter().collect::<BTreeMap<_, _>>(),
            },
            license: ManifestLicense {
                urdf: "BSD-3-Clause".into(),
                meshes: "LicenseRef-Vendor".into(),
                meshes_redistributable: redistributable,
                notes: String::new(),
            },
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn scene_roundtrips(s in scene()) { roundtrip(&s); }

    #[test]
    fn scenario_roundtrips(s in scenario()) { roundtrip(&s); }

    #[test]
    fn baked_roundtrips(b in baked()) { roundtrip(&b); }

    #[test]
    fn manifest_roundtrips(m in manifest()) { roundtrip(&m); }
}

#[test]
fn unknown_fields_are_rejected() {
    let scene = r#"{"version": 1, "cameras": [], "camras": []}"#;
    assert!(serde_json::from_str::<SceneSpec>(scene).is_err());
    let rig = r#"{"version": 1, "rigs": [{"id": "rig0", "parent": "world",
        "parent_se3_self": {"rotation": [0,0,0,1], "translation": [0,0,0]}, "colour": 1}]}"#;
    assert!(serde_json::from_str::<SceneSpec>(rig).is_err());
    let step =
        r#"{"version": 1, "dt": 0.01, "steps": [{"type": "wait", "duration_s": 1, "x": 0}]}"#;
    assert!(serde_json::from_str::<ScenarioSpec>(step).is_err());
}

#[test]
fn se3_wire_format_is_rotation_then_translation() {
    // The calibration-rs / nalgebra `Isometry3` wire shape, pinned here too:
    // quaternion [qx, qy, qz, qw] scalar-last, translation in metres.
    let frame = FrameSpec {
        id: "f".into(),
        parent: FrameRef::world(),
        parent_se3_self: Isometry3::from_parts(
            Translation3::new(1.0, 2.0, 3.0),
            UnitQuaternion::from_quaternion(Quaternion::new(0.5, 0.5, 0.5, 0.5)),
        ),
    };
    let v = serde_json::to_value(&frame).unwrap();
    assert_eq!(
        v["parent_se3_self"],
        serde_json::json!({"rotation": [0.5, 0.5, 0.5, 0.5], "translation": [1.0, 2.0, 3.0]})
    );
    let rot = serde_json::json!({"rotation": [0.0, 0.0, 1.0, 0.0], "translation": [0.0, 0.0, 0.0]});
    let iso: Isometry3<f64> = serde_json::from_value(rot).unwrap();
    // [qx, qy, qz, qw] = [0, 0, 1, 0] is a half-turn about +Z.
    let p = iso * nalgebra::Point3::new(1.0, 0.0, 0.0);
    approx::assert_relative_eq!(p, nalgebra::Point3::new(-1.0, 0.0, 0.0), epsilon = 1e-15);
}

#[test]
fn defaults_fill_optional_fields() {
    let scene: SceneSpec = serde_json::from_str(r#"{"version": 1}"#).unwrap();
    assert!(scene.cameras.is_empty() && scene.robots.is_empty());
    let scenario: ScenarioSpec = serde_json::from_str(
        r#"{"version": 1, "dt": 0.01, "steps": [{"type": "ptp_joints", "robot": "r", "q": [0]}]}"#,
    )
    .unwrap();
    let Step::PtpJoints { speed_scale, .. } = scenario.steps[0] else {
        panic!("expected ptp_joints");
    };
    assert_eq!(speed_scale, 1.0);
}
