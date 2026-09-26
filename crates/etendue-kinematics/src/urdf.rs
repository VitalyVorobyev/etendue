//! URDF → kinematic tree, bound to a [`RobotManifest`].
//!
//! Supported joint types: `revolute`, `continuous` (a revolute joint whose
//! limits come from the manifest), `prismatic`, and `fixed`. `floating`,
//! `planar`, and `spherical` joints and `<mimic>` are rejected.
//!
//! URDF semantics: a joint's `<origin>` is the pose of the joint frame in the
//! parent link frame; the child link frame coincides with the joint frame
//! after the joint motion (rotation about, or translation along, `<axis>`,
//! which is expressed in the joint frame). `rpy` is fixed-axis X-Y-Z, i.e.
//! `R = Rz(yaw) · Ry(pitch) · Rx(roll)`.

use std::collections::HashMap;

use etendue_scene::RobotManifest;
use nalgebra::{Isometry3, Translation3, Unit, UnitQuaternion, Vector3};

use crate::error::{Error, Result};

/// The motion a joint allows.
#[derive(Clone, Debug)]
pub enum JointKind {
    /// No motion.
    Fixed,
    /// Rotation about a unit axis (joint frame), angle `q` in radians.
    Revolute {
        /// Unit rotation axis in the joint frame.
        axis: Unit<Vector3<f64>>,
    },
    /// Translation along a unit axis (joint frame), distance `q` in metres.
    Prismatic {
        /// Unit translation axis in the joint frame.
        axis: Unit<Vector3<f64>>,
    },
}

/// One URDF joint.
#[derive(Clone, Debug)]
pub struct Joint {
    /// URDF joint name.
    pub name: String,
    /// Parent link index.
    pub parent: usize,
    /// Child link index.
    pub child: usize,
    /// Pose of the joint frame in the parent link (`parent_se3_joint`).
    pub origin: Isometry3<f64>,
    /// Joint motion.
    pub kind: JointKind,
}

impl Joint {
    /// `parent_se3_child` at joint position `q` (ignored for fixed joints).
    #[must_use]
    pub fn transform(&self, q: f64) -> Isometry3<f64> {
        match &self.kind {
            JointKind::Fixed => self.origin,
            JointKind::Revolute { axis } => self.origin * UnitQuaternion::from_axis_angle(axis, q),
            JointKind::Prismatic { axis } => {
                self.origin * Translation3::from(axis.into_inner() * q)
            }
        }
    }
}

/// A commanded joint: one entry of the joint vector `q`, with its limits.
#[derive(Clone, Debug)]
pub struct ActiveJoint {
    /// Index into [`RobotModel::joints`].
    pub joint: usize,
    /// Joint name.
    pub name: String,
    /// Lower position limit.
    pub lower: f64,
    /// Upper position limit.
    pub upper: f64,
    /// Maximum speed, `> 0`.
    pub max_velocity: f64,
    /// Maximum acceleration, `> 0`.
    pub max_acceleration: f64,
}

/// A robot's kinematic tree bound to its manifest.
///
/// The joint vector `q` is ordered as the manifest's `joints` — exactly the
/// movable joints on the path `base_link → tcp_link`. Movable joints off that
/// path (e.g. gripper fingers) are held at zero.
///
/// `base_link` need not be an ancestor of `tcp_link`: REP-199 descriptions
/// hang the controller's `base` frame off `base_link` with a fixed joint. The
/// path from `base_link` up to the common ancestor must then consist of fixed
/// joints only.
#[derive(Clone, Debug)]
pub struct RobotModel {
    pub(crate) id: String,
    pub(crate) links: Vec<String>,
    pub(crate) joints: Vec<Joint>,
    /// Joint whose child is the link, per link (`None` for the root).
    pub(crate) parent_joint: Vec<Option<usize>>,
    /// Links in topological order, root first.
    pub(crate) topo: Vec<usize>,
    pub(crate) base: usize,
    pub(crate) tcp: usize,
    /// Pose of the common ancestor of base and TCP in the base frame (a
    /// constant: the path between them is fixed).
    pub(crate) base_se3_ancestor: Isometry3<f64>,
    /// Joint indices on the path common ancestor → tcp, in order (fixed
    /// included).
    pub(crate) chain: Vec<usize>,
    /// Commanded joints, in `q` order.
    pub(crate) active: Vec<ActiveJoint>,
    /// `q` index of each joint (`None` for fixed or off-path joints).
    pub(crate) q_index: Vec<Option<usize>>,
}

fn pose(p: &urdf_rs::Pose) -> Isometry3<f64> {
    Isometry3::from_parts(
        Translation3::new(p.xyz[0], p.xyz[1], p.xyz[2]),
        UnitQuaternion::from_euler_angles(p.rpy[0], p.rpy[1], p.rpy[2]),
    )
}

fn axis(joint: &urdf_rs::Joint) -> Result<Unit<Vector3<f64>>> {
    let v = Vector3::new(joint.axis.xyz[0], joint.axis.xyz[1], joint.axis.xyz[2]);
    Unit::try_new(v, 1e-12).ok_or_else(|| {
        Error::InvalidModel(format!("joint `{}` has a zero-length axis", joint.name))
    })
}

impl RobotModel {
    /// Parse `urdf` and bind it to `manifest`.
    ///
    /// # Errors
    ///
    /// - [`Error::Urdf`] if the XML does not parse;
    /// - [`Error::Validation`] if the manifest fails
    ///   [`RobotManifest::validate`];
    /// - [`Error::InvalidModel`] if the URDF is not a single tree, uses an
    ///   unsupported joint type or `<mimic>`, if `base_link` / `tcp_link` are
    ///   missing, if the path from `base_link` to the common ancestor with
    ///   `tcp_link` contains a movable joint, or if the manifest joints are
    ///   not exactly the movable joints on the path, in order.
    pub fn from_urdf_str(urdf: &str, manifest: &RobotManifest) -> Result<Self> {
        manifest.validate()?;
        let robot = urdf_rs::read_from_string(urdf).map_err(|e| Error::Urdf(e.to_string()))?;

        let links: Vec<String> = robot.links.iter().map(|l| l.name.clone()).collect();
        let mut link_index: HashMap<&str, usize> = HashMap::new();
        for (i, l) in links.iter().enumerate() {
            if link_index.insert(l.as_str(), i).is_some() {
                return Err(Error::InvalidModel(format!("duplicate link `{l}`")));
            }
        }
        let find = |name: &str, what: &str| {
            link_index
                .get(name)
                .copied()
                .ok_or_else(|| Error::InvalidModel(format!("{what} `{name}` is not a URDF link")))
        };

        let mut joints = Vec::with_capacity(robot.joints.len());
        let mut parent_joint: Vec<Option<usize>> = vec![None; links.len()];
        for (j, uj) in robot.joints.iter().enumerate() {
            if uj.mimic.is_some() {
                return Err(Error::InvalidModel(format!(
                    "joint `{}`: <mimic> is not supported",
                    uj.name
                )));
            }
            let kind = match uj.joint_type {
                urdf_rs::JointType::Fixed => JointKind::Fixed,
                urdf_rs::JointType::Revolute | urdf_rs::JointType::Continuous => {
                    JointKind::Revolute { axis: axis(uj)? }
                }
                urdf_rs::JointType::Prismatic => JointKind::Prismatic { axis: axis(uj)? },
                ref other => {
                    return Err(Error::InvalidModel(format!(
                        "joint `{}`: unsupported joint type {other:?}",
                        uj.name
                    )));
                }
            };
            let parent = find(&uj.parent.link, "joint parent")?;
            let child = find(&uj.child.link, "joint child")?;
            if parent_joint[child].replace(j).is_some() {
                return Err(Error::InvalidModel(format!(
                    "link `{}` has more than one parent joint",
                    links[child]
                )));
            }
            joints.push(Joint {
                name: uj.name.clone(),
                parent,
                child,
                origin: pose(&uj.origin),
                kind,
            });
        }

        let roots: Vec<usize> = (0..links.len())
            .filter(|&l| parent_joint[l].is_none())
            .collect();
        if roots.len() != 1 {
            return Err(Error::InvalidModel(format!(
                "URDF must be a single tree, found {} root links",
                roots.len()
            )));
        }
        // Topological order: breadth-first from the root.
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); links.len()];
        for j in &joints {
            children[j.parent].push(j.child);
        }
        let mut topo = vec![roots[0]];
        let mut k = 0;
        while k < topo.len() {
            topo.extend_from_slice(&children[topo[k]]);
            k += 1;
        }
        if topo.len() != links.len() {
            return Err(Error::InvalidModel("URDF joint graph has a cycle".into()));
        }

        let base = find(&manifest.base_link, "base_link")?;
        let tcp = find(&manifest.tcp_link, "tcp_link")?;
        let ancestors = |mut link: usize| {
            let mut out = vec![link];
            while let Some(j) = parent_joint[link] {
                link = joints[j].parent;
                out.push(link);
            }
            out
        };
        let tcp_up = ancestors(tcp);
        let base_up = ancestors(base);
        let ancestor = *base_up
            .iter()
            .find(|l| tcp_up.contains(l))
            .expect("a single tree has a common ancestor");
        // base → ancestor must be rigid.
        let mut ancestor_se3_base = Isometry3::identity();
        for &l in base_up.iter().take_while(|&&l| l != ancestor) {
            let j = parent_joint[l].expect("below the ancestor");
            if !matches!(joints[j].kind, JointKind::Fixed) {
                return Err(Error::InvalidModel(format!(
                    "base_link `{}` is connected to tcp_link `{}` through movable joint `{}` \
                     above the base",
                    manifest.base_link, manifest.tcp_link, joints[j].name
                )));
            }
            ancestor_se3_base = joints[j].origin * ancestor_se3_base;
        }
        let mut chain: Vec<usize> = tcp_up
            .iter()
            .take_while(|&&l| l != ancestor)
            .map(|&l| parent_joint[l].expect("below the ancestor"))
            .collect();
        chain.reverse();

        let movable: Vec<usize> = chain
            .iter()
            .copied()
            .filter(|&j| !matches!(joints[j].kind, JointKind::Fixed))
            .collect();
        let movable_names: Vec<&str> = movable.iter().map(|&j| joints[j].name.as_str()).collect();
        let manifest_names: Vec<&str> = manifest.joints.iter().map(|j| j.name.as_str()).collect();
        if movable_names != manifest_names {
            return Err(Error::InvalidModel(format!(
                "manifest joints {manifest_names:?} must equal the movable joints on the path \
                 {} → {}: {movable_names:?}",
                manifest.base_link, manifest.tcp_link
            )));
        }

        let mut q_index = vec![None; joints.len()];
        let active = movable
            .iter()
            .zip(&manifest.joints)
            .enumerate()
            .map(|(i, (&j, mj))| {
                q_index[j] = Some(i);
                ActiveJoint {
                    joint: j,
                    name: mj.name.clone(),
                    lower: mj.lower,
                    upper: mj.upper,
                    max_velocity: mj.max_velocity,
                    max_acceleration: mj.max_acceleration,
                }
            })
            .collect();

        Ok(Self {
            id: manifest.id.clone(),
            links,
            joints,
            parent_joint,
            topo,
            base,
            tcp,
            base_se3_ancestor: ancestor_se3_base.inverse(),
            chain,
            active,
            q_index,
        })
    }

    /// Robot model id (from the manifest).
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// URDF link names; link indices used elsewhere index this slice.
    #[must_use]
    pub fn links(&self) -> &[String] {
        &self.links
    }

    /// URDF joints.
    #[must_use]
    pub fn joints(&self) -> &[Joint] {
        &self.joints
    }

    /// Commanded joints in `q` order.
    #[must_use]
    pub fn active_joints(&self) -> &[ActiveJoint] {
        &self.active
    }

    /// Degrees of freedom (length of `q`).
    #[must_use]
    pub fn dof(&self) -> usize {
        self.active.len()
    }

    /// Index of the base link.
    #[must_use]
    pub fn base_link(&self) -> usize {
        self.base
    }

    /// Index of the TCP link.
    #[must_use]
    pub fn tcp_link(&self) -> usize {
        self.tcp
    }

    /// Index of a link by name.
    #[must_use]
    pub fn link_index(&self, name: &str) -> Option<usize> {
        self.links.iter().position(|l| l == name)
    }

    /// Check that `q` has [`dof`](Self::dof) finite entries inside the joint
    /// limits (with an absolute slack `tol`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidJoints`] describing the first violation.
    pub fn check_q(&self, q: &[f64], tol: f64) -> Result<()> {
        if q.len() != self.dof() {
            return Err(Error::InvalidJoints(format!(
                "robot `{}` has {} joints, got {} values",
                self.id,
                self.dof(),
                q.len()
            )));
        }
        for (v, j) in q.iter().zip(&self.active) {
            if !v.is_finite() || *v < j.lower - tol || *v > j.upper + tol {
                return Err(Error::InvalidJoints(format!(
                    "joint `{}` = {v} is outside [{}, {}]",
                    j.name, j.lower, j.upper
                )));
            }
        }
        Ok(())
    }
}
