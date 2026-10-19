//! Forward kinematics and the geometric Jacobian.
//!
//! All poses are relative to the robot's **base link** (`base_se3_x`), not
//! the URDF root, so a `world` dummy root in the URDF has no effect.

use nalgebra::{DMatrix, Isometry3, Matrix6, Vector3};

use crate::urdf::{JointKind, RobotModel};

impl RobotModel {
    /// `base_se3_link` for every URDF link (indexed like
    /// [`RobotModel::links`]) at joint vector `q`.
    ///
    /// # Panics
    ///
    /// If `q.len() != self.dof()`.
    #[must_use]
    pub fn link_poses(&self, q: &[f64]) -> Vec<Isometry3<f64>> {
        assert_eq!(q.len(), self.dof(), "joint vector length");
        let mut root_se3: Vec<Isometry3<f64>> = vec![Isometry3::identity(); self.links.len()];
        for &link in &self.topo[1..] {
            let j = &self.joints[self.parent_joint[link].expect("non-root link has a parent")];
            let qj = self.q_index[self.parent_joint[link].unwrap()].map_or(0.0, |i| q[i]);
            root_se3[link] = root_se3[j.parent] * j.transform(qj);
        }
        let base_se3_root = root_se3[self.base].inverse();
        root_se3.iter().map(|p| base_se3_root * p).collect()
    }

    /// `base_se3_tcp` at joint vector `q` (walks only the joints between the
    /// base and the TCP).
    ///
    /// # Panics
    ///
    /// If `q.len() != self.dof()`.
    #[must_use]
    pub fn tcp_pose(&self, q: &[f64]) -> Isometry3<f64> {
        assert_eq!(q.len(), self.dof(), "joint vector length");
        self.chain.iter().fold(self.base_se3_ancestor, |acc, &j| {
            acc * self.joints[j].transform(self.q_index[j].map_or(0.0, |i| q[i]))
        })
    }

    /// `base_se3_tcp` and the 6 × dof geometric Jacobian of the TCP in the
    /// base frame: rows 0–2 linear velocity of the TCP origin, rows 3–5
    /// angular velocity.
    ///
    /// # Panics
    ///
    /// If `q.len() != self.dof()`.
    #[must_use]
    pub fn tcp_jacobian(&self, q: &[f64]) -> (Isometry3<f64>, DMatrix<f64>) {
        assert_eq!(q.len(), self.dof(), "joint vector length");
        // Joint frames (after origin, before motion) and the TCP pose.
        let mut acc = self.base_se3_ancestor;
        let mut axes: Vec<(usize, Vector3<f64>, Vector3<f64>, bool)> =
            Vec::with_capacity(self.dof());
        for &j in &self.chain {
            let joint = &self.joints[j];
            let qj = self.q_index[j].map_or(0.0, |i| q[i]);
            let joint_frame = acc * joint.origin;
            match (&joint.kind, self.q_index[j]) {
                (JointKind::Revolute { axis }, Some(i)) => {
                    axes.push((
                        i,
                        joint_frame.rotation * axis.into_inner(),
                        joint_frame.translation.vector,
                        true,
                    ));
                }
                (JointKind::Prismatic { axis }, Some(i)) => {
                    axes.push((
                        i,
                        joint_frame.rotation * axis.into_inner(),
                        joint_frame.translation.vector,
                        false,
                    ));
                }
                _ => {}
            }
            acc *= joint.transform(qj);
        }
        let p_tcp = acc.translation.vector;
        let mut jac = DMatrix::zeros(6, self.dof());
        for (i, z, p, revolute) in axes {
            if revolute {
                jac.fixed_view_mut::<3, 1>(0, i)
                    .copy_from(&z.cross(&(p_tcp - p)));
                jac.fixed_view_mut::<3, 1>(3, i).copy_from(&z);
            } else {
                jac.fixed_view_mut::<3, 1>(0, i).copy_from(&z);
            }
        }
        (acc, jac)
    }
}

/// `J·Jᵀ` of a 6 × n Jacobian, summed in order. nalgebra hands dynamic products to
/// `matrixmultiply`, whose native kernels fuse multiply-adds that its wasm path does not, so IK
/// would differ by an ulp between the two (gate G12.1).
pub(crate) fn jjt(jac: &DMatrix<f64>) -> Matrix6<f64> {
    Matrix6::from_fn(|r, c| (0..jac.ncols()).fold(0.0, |acc, k| acc + jac[(r, k)] * jac[(c, k)]))
}

impl RobotModel {
    /// Yoshikawa's manipulability at `q`: `√det(J Jᵀ)` (`√det(JᵀJ)` with fewer than six
    /// joints), zero at a singularity.
    ///
    /// # Panics
    ///
    /// If `q.len() != self.dof()`.
    #[must_use]
    pub fn manipulability(&self, q: &[f64]) -> f64 {
        let (_, jac) = self.tcp_jacobian(q);
        let det = if jac.ncols() >= 6 {
            jjt(&jac).determinant()
        } else {
            let n = jac.ncols();
            DMatrix::from_fn(n, n, |r, c| {
                (0..6).fold(0.0, |acc, k| acc + jac[(k, r)] * jac[(k, c)])
            })
            .determinant()
        };
        det.max(0.0).sqrt()
    }
}
