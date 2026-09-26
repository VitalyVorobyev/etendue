//! URDF loading, forward kinematics, Jacobian, and IK on hand-written models.

mod common;

/// Agreement between two floating-point evaluation paths (different
/// operation order; the platform libm's `sin`/`cos` differ by ULPs, e.g.
/// MSVC vs glibc/macOS). Far below every gate (G1.1: 1e-9).
const TOL: f64 = 1e-12;

use approx::assert_relative_eq;
use common::{Rng, pose_distance, test_arm};
use etendue_kinematics::{Error, IkOptions, RobotModel};
use etendue_scene::RobotManifest;
use nalgebra::{Isometry3, Point3, Translation3, UnitQuaternion};

fn manifest(joints: &[&str], base: &str, tcp: &str) -> RobotManifest {
    serde_json::from_value(serde_json::json!({
        "version": 1, "id": "m", "name": "m", "urdf": "m.urdf",
        "base_link": base, "tcp_link": tcp,
        "joints": joints.iter().map(|n| serde_json::json!({
            "name": n, "lower": -3.0, "upper": 3.0, "max_velocity": 1.0,
            "max_acceleration": 1.0, "limit_source": "test"
        })).collect::<Vec<_>>(),
        "visuals": [],
        "source": {"repository": "", "revision": "", "entry": "", "xacro_args": {}},
        "license": {"urdf": "MIT", "meshes": "NONE", "meshes_redistributable": true, "notes": ""}
    }))
    .unwrap()
}

const RP_ARM: &str = r#"<robot name="rp">
  <link name="base"/><link name="l1"/><link name="tcp"/>
  <joint name="r" type="revolute"><parent link="base"/><child link="l1"/>
    <origin xyz="0 0 0.5"/><axis xyz="0 0 1"/><limit lower="-3" upper="3" effort="1" velocity="1"/></joint>
  <joint name="p" type="prismatic"><parent link="l1"/><child link="tcp"/>
    <origin xyz="1 0 0"/><axis xyz="1 0 0"/><limit lower="0" upper="1" effort="1" velocity="1"/></joint>
</robot>"#;

#[test]
fn revolute_prismatic_fk_matches_closed_form() {
    let model = RobotModel::from_urdf_str(RP_ARM, &manifest(&["r", "p"], "base", "tcp")).unwrap();
    for (q1, q2) in [(0.0, 0.0), (0.7, 0.3), (-2.1, 0.9)] {
        let pose = model.tcp_pose(&[q1, q2]);
        let r = 1.0 + q2;
        assert_relative_eq!(
            pose * Point3::origin(),
            Point3::new(r * q1.cos(), r * q1.sin(), 0.5),
            epsilon = TOL
        );
        assert_relative_eq!(pose.rotation.angle(), q1.abs(), epsilon = TOL);
    }
}

#[test]
fn link_poses_are_relative_to_the_base_not_the_urdf_root() {
    let model = test_arm();
    let q = vec![0.0; 6];
    let poses = model.link_poses(&q);
    let base = model.base_link();
    assert_relative_eq!(
        poses[base].to_homogeneous(),
        Isometry3::identity().to_homogeneous(),
        epsilon = TOL
    );
    // The `world` root sits at the inverse of world_joint's origin.
    let world = model.link_index("world").unwrap();
    let world_se3_base = Isometry3::from_parts(
        Translation3::new(0.1, -0.2, 0.3),
        UnitQuaternion::from_euler_angles(0.0, 0.0, 0.5),
    );
    assert_relative_eq!(
        poses[world].to_homogeneous(),
        world_se3_base.inverse().to_homogeneous(),
        epsilon = TOL
    );
    // The chain walk and the whole-tree walk agree on the TCP.
    let mut rng = Rng::new(7);
    for _ in 0..100 {
        let q = rng.q(&model, 0.0);
        let (dt, dr) = pose_distance(&model.link_poses(&q)[model.tcp_link()], &model.tcp_pose(&q));
        assert!(dt < TOL && dr < TOL, "{dt} {dr}");
    }
}

#[test]
fn off_chain_movable_joints_are_held_at_zero() {
    let model = test_arm();
    let q = vec![0.3, -0.4, 0.5, 0.6, -0.7, 0.8];
    let poses = model.link_poses(&q);
    let finger = poses[model.link_index("finger").unwrap()];
    let tool = poses[model.tcp_link()];
    assert_relative_eq!(
        (tool.inverse() * finger).to_homogeneous(),
        Isometry3::translation(0.0, 0.0, 0.05).to_homogeneous(),
        epsilon = TOL
    );
}

#[test]
fn jacobian_matches_finite_differences() {
    let model = test_arm();
    let mut rng = Rng::new(11);
    for _ in 0..50 {
        let q = rng.q(&model, 0.1);
        let (_, jac) = model.tcp_jacobian(&q);
        let h = 1e-7;
        for i in 0..model.dof() {
            let mut qp = q.clone();
            qp[i] += h;
            let mut qm = q.clone();
            qm[i] -= h;
            let (pp, pm) = (model.tcp_pose(&qp), model.tcp_pose(&qm));
            let v = (pp.translation.vector - pm.translation.vector) / (2.0 * h);
            let w = (pp.rotation * pm.rotation.inverse()).scaled_axis() / (2.0 * h);
            assert_relative_eq!(jac.fixed_view::<3, 1>(0, i).into_owned(), v, epsilon = 1e-7);
            assert_relative_eq!(jac.fixed_view::<3, 1>(3, i).into_owned(), w, epsilon = 1e-7);
        }
    }
}

#[test]
fn ik_round_trips_from_nearby_and_random_seeds() {
    let model = test_arm();
    let opts = IkOptions::default();
    let mut rng = Rng::new(42);
    let (mut near_fail, mut far_fail) = (0, 0);
    let n = 300;
    for _ in 0..n {
        let q_true = rng.q(&model, 0.2);
        let target = model.tcp_pose(&q_true);
        let near: Vec<f64> = q_true.iter().map(|v| v + rng.uniform(-0.2, 0.2)).collect();
        let far = rng.q(&model, 0.0);
        for (seed, fails) in [(&near, &mut near_fail), (&far, &mut far_fail)] {
            match model.ik(&target, seed, &opts) {
                Ok(sol) => {
                    let (dt, dr) = pose_distance(&model.tcp_pose(&sol.q), &target);
                    assert!(dt <= 1e-9 && dr <= 1e-9, "residual {dt} m, {dr} rad");
                    model.check_q(&sol.q, 0.0).expect("solution within limits");
                }
                Err(Error::IkFailed(_)) => *fails += 1,
                Err(e) => panic!("{e}"),
            }
        }
    }
    assert_eq!(near_fail, 0, "IK from nearby seeds must always converge");
    assert!(far_fail * 5 < n, "{far_fail}/{n} random-seed failures");
}

#[test]
fn ik_rejects_unreachable_poses() {
    let model = test_arm();
    let far_away = Isometry3::translation(10.0, 0.0, 0.0);
    assert!(matches!(
        model.ik(&far_away, &[0.0; 6], &IkOptions::default()),
        Err(Error::IkFailed(_))
    ));
}

#[test]
fn unsupported_models_are_rejected() {
    let floating = RP_ARM.replace(r#"type="prismatic""#, r#"type="floating""#);
    assert!(matches!(
        RobotModel::from_urdf_str(&floating, &manifest(&["r", "p"], "base", "tcp")),
        Err(Error::InvalidModel(_))
    ));
    // Manifest joints must be exactly the movable path joints, in order.
    for joints in [&["p", "r"][..], &["r"][..]] {
        let err = RobotModel::from_urdf_str(RP_ARM, &manifest(joints, "base", "tcp")).unwrap_err();
        assert!(
            err.to_string().contains("movable joints on the path"),
            "{err}"
        );
    }
    // tcp_link must descend from base_link.
    let err = RobotModel::from_urdf_str(RP_ARM, &manifest(&[], "tcp", "base"));
    assert!(err.is_err());
    // Mimic joints are not supported.
    let mimic = RP_ARM.replace(
        r#"<axis xyz="1 0 0"/>"#,
        r#"<axis xyz="1 0 0"/><mimic joint="r" multiplier="1" offset="0"/>"#,
    );
    let err = RobotModel::from_urdf_str(&mimic, &manifest(&["r", "p"], "base", "tcp")).unwrap_err();
    assert!(err.to_string().contains("mimic"), "{err}");
}

#[test]
fn check_q_enforces_length_and_limits() {
    let model = test_arm();
    assert!(model.check_q(&[0.0; 5], 0.0).is_err());
    assert!(model.check_q(&[0.0, 0.0, 0.0, 0.0, 0.0, 4.0], 0.0).is_err());
    assert!(
        model
            .check_q(&[0.0, 0.0, 0.0, 0.0, 0.0, f64::NAN], 0.0)
            .is_err()
    );
    model.check_q(&[0.0; 6], 0.0).unwrap();
}

#[test]
fn base_may_hang_off_the_chain_through_fixed_joints() {
    // REP-199 layout: `base` is a fixed child of `base_link`, rotated Rz(π),
    // and not an ancestor of the TCP.
    let urdf = RP_ARM.replace(
        "</robot>",
        r#"<link name="ctrl_base"/>
  <joint name="base-ctrl" type="fixed"><parent link="base"/><child link="ctrl_base"/>
    <origin xyz="0 0 0.1" rpy="0 0 3.141592653589793"/></joint>
</robot>"#,
    );
    let rep = RobotModel::from_urdf_str(&urdf, &manifest(&["r", "p"], "ctrl_base", "tcp")).unwrap();
    let plain = RobotModel::from_urdf_str(RP_ARM, &manifest(&["r", "p"], "base", "tcp")).unwrap();
    let base_se3_ctrl = Isometry3::from_parts(
        Translation3::new(0.0, 0.0, 0.1),
        UnitQuaternion::from_euler_angles(0.0, 0.0, std::f64::consts::PI),
    );
    let q = [0.4, 0.2];
    let expected = base_se3_ctrl.inverse() * plain.tcp_pose(&q);
    let (dt, dr) = pose_distance(&rep.tcp_pose(&q), &expected);
    assert!(dt < TOL && dr < TOL, "{dt} {dr}");
    let (dt, dr) = pose_distance(&rep.link_poses(&q)[rep.tcp_link()], &expected);
    assert!(dt < TOL && dr < TOL, "{dt} {dr}");
    // Jacobians agree after rotating into the controller base frame.
    let (_, j_rep) = rep.tcp_jacobian(&q);
    let (_, j_plain) = plain.tcp_jacobian(&q);
    let r = base_se3_ctrl.rotation.inverse().to_rotation_matrix();
    for i in 0..2 {
        assert_relative_eq!(
            j_rep.fixed_view::<3, 1>(0, i).into_owned(),
            r * j_plain.fixed_view::<3, 1>(0, i).into_owned(),
            epsilon = TOL
        );
    }
}
