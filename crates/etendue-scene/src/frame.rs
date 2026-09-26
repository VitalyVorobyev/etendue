//! Frame references, named auxiliary frames, and the resolved frame graph
//! ([ADR 0002](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0002-frame-tree.md)).
//!
//! Every placeable entity names a parent frame and carries
//! `parent_se3_self` (maps self coordinates into the parent frame). A
//! [`FrameRef`] is one of:
//!
//! - `"world"` — the root: right-handed, +Z up, metres;
//! - `"<robot_id>/<link_name>"` — a link of a robot (link names come from the
//!   robot's URDF);
//! - `"<entity_id>"` — any other entity (a frame, rig, camera, …).
//!
//! A bare robot id is **not** a frame: attach to one of its links instead.
//!
//! [`FrameGraph`] resolves the attachment tree once (topological order, cycle
//! and dangling-parent checks) and then maps per-sample robot link poses to
//! `world_se3_frame` for every frame.

use std::collections::BTreeMap;
use std::fmt;

use nalgebra::Isometry3;
use serde::{Deserialize, Serialize};

use crate::SceneSpec;
use crate::validate::{Issues, ValidationError};

/// Name of the root frame.
pub const WORLD: &str = "world";

/// A reference to a frame: `"world"`, `"<robot_id>/<link_name>"`, or
/// `"<entity_id>"`. Parse it with [`FrameRef::parse`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct FrameRef(pub String);

impl FrameRef {
    /// The world frame.
    #[must_use]
    pub fn world() -> Self {
        Self(WORLD.to_owned())
    }

    /// A robot link frame, `"<robot>/<link>"`.
    #[must_use]
    pub fn robot_link(robot: &str, link: &str) -> Self {
        Self(format!("{robot}/{link}"))
    }

    /// An entity frame, `"<id>"`.
    #[must_use]
    pub fn entity(id: &str) -> Self {
        Self(id.to_owned())
    }

    /// The raw string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Split the reference into its grammatical form.
    ///
    /// # Errors
    ///
    /// Returns a message if the string is empty, has an empty robot or link
    /// part, or names an entity id with characters outside
    /// `[A-Za-z0-9_-]`.
    pub fn parse(&self) -> Result<ParsedFrameRef<'_>, String> {
        let s = self.0.as_str();
        if s == WORLD {
            return Ok(ParsedFrameRef::World);
        }
        match s.split_once('/') {
            Some((robot, link)) => {
                if !is_valid_id(robot) {
                    return Err(format!(
                        "frame `{s}`: robot id `{robot}` is not a valid id ([A-Za-z0-9_-]+, not `world`)"
                    ));
                }
                if link.is_empty() {
                    return Err(format!("frame `{s}`: empty link name"));
                }
                Ok(ParsedFrameRef::RobotLink { robot, link })
            }
            None if is_valid_id(s) => Ok(ParsedFrameRef::Entity(s)),
            None => Err(format!(
                "frame `{s}` is not `world`, `<robot>/<link>`, or a valid entity id"
            )),
        }
    }
}

impl fmt::Display for FrameRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for FrameRef {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// The grammatical form of a [`FrameRef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedFrameRef<'a> {
    /// `"world"`.
    World,
    /// `"<robot>/<link>"`.
    RobotLink {
        /// Robot id.
        robot: &'a str,
        /// URDF link name.
        link: &'a str,
    },
    /// `"<entity_id>"`.
    Entity(&'a str),
}

/// Whether `id` is a valid robot/entity id: non-empty ASCII
/// `[A-Za-z0-9_-]`, and not `"world"`.
#[must_use]
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id != WORLD
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A named auxiliary frame — a fixture, a mounting plate, a TCP offset.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FrameSpec {
    /// Unique id (shared namespace with every other entity and robot).
    pub id: String,
    /// Parent frame.
    pub parent: FrameRef,
    /// Pose of this frame in its parent (maps self → parent).
    #[cfg_attr(
        feature = "schemars",
        schemars(with = "vision_calibration_core::Iso3Schema")
    )]
    pub parent_se3_self: Isometry3<f64>,
}

#[derive(Clone, Debug)]
enum NodeKind {
    World,
    Fixed {
        parent: usize,
        parent_se3_self: Isometry3<f64>,
    },
    RobotLink {
        robot: usize,
        link: usize,
    },
}

#[derive(Clone, Debug)]
struct Node {
    name: String,
    kind: NodeKind,
}

#[derive(Clone, Debug)]
struct RobotMount {
    mount_parent: usize,
    parent_se3_base: Isometry3<f64>,
}

/// The resolved attachment tree of a scene.
///
/// Built once per scene with [`FrameGraph::build`]; [`FrameGraph::resolve`]
/// then turns robot link poses (from forward kinematics) into
/// `world_se3_frame` for every frame, in [`FrameGraph::names`] order.
///
/// Frames are listed in **topological order**: `"world"` first, and every
/// frame after its parent. Among frames whose parents are ready, declaration
/// order is kept (auxiliary frames, robot links, rigs, cameras, lasers,
/// lights, targets, parts), so the order is deterministic.
#[derive(Clone, Debug)]
pub struct FrameGraph {
    nodes: Vec<Node>,
    index: BTreeMap<String, usize>,
    robots: Vec<RobotMount>,
}

/// A frame to be placed, before sorting.
struct Pending {
    name: String,
    path: String,
    parent: FrameRef,
    what: PendingKind,
}

enum PendingKind {
    Fixed(Isometry3<f64>),
    RobotLink { robot: usize, link: usize },
}

impl FrameGraph {
    /// Resolve the attachment tree of `scene`.
    ///
    /// `robot_links[i]` lists every URDF link name of `scene.robots[i]`
    /// (from the robot model); a link frame `"<robot>/<link>"` is created for
    /// each.
    ///
    /// # Errors
    ///
    /// Returns every problem found: `robot_links` not aligned with
    /// `scene.robots`, unparsable or dangling parents (unknown entity, robot,
    /// or link; a bare robot id), and attachment cycles.
    pub fn build(scene: &SceneSpec, robot_links: &[Vec<String>]) -> Result<Self, ValidationError> {
        let mut issues = Issues::default();
        if robot_links.len() != scene.robots.len() {
            issues.push(
                "robots",
                format!(
                    "{} robot link list(s) supplied for {} robot(s)",
                    robot_links.len(),
                    scene.robots.len()
                ),
            );
            return Err(issues.into_error());
        }

        let mut pending: Vec<Pending> = Vec::new();
        for mount in scene.mounted() {
            if mount.kind == "robots" {
                continue;
            }
            pending.push(Pending {
                name: mount.id.to_owned(),
                path: mount.path(),
                parent: mount.parent.clone(),
                what: PendingKind::Fixed(*mount.parent_se3_self),
            });
        }
        // Robot links, grouped per robot, right after auxiliary frames so a
        // robot mounted on a fixture resolves in declaration order.
        let n_frames = scene.frames.len();
        let mut link_pending = Vec::new();
        for (r, (robot, links)) in scene.robots.iter().zip(robot_links).enumerate() {
            for (l, link) in links.iter().enumerate() {
                link_pending.push(Pending {
                    name: format!("{}/{link}", robot.id),
                    path: format!("robots[{r}].parent"),
                    parent: robot.parent.clone(),
                    what: PendingKind::RobotLink { robot: r, link: l },
                });
            }
        }
        pending.splice(n_frames..n_frames, link_pending);

        // Every frame name that will exist, for dangling-parent checks.
        let mut known: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, p) in pending.iter().enumerate() {
            if known.insert(p.name.as_str(), i).is_some() {
                issues.push(p.path.clone(), format!("duplicate frame name `{}`", p.name));
            }
        }
        let robot_ids: BTreeMap<&str, usize> = scene
            .robots
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id.as_str(), i))
            .collect();

        // Parent of each pending frame: None = world, Some(i) = pending[i].
        let mut parent_of: Vec<Option<Option<usize>>> = Vec::with_capacity(pending.len());
        for p in &pending {
            let resolved = match p.parent.parse() {
                Err(msg) => {
                    issues.push(p.path.clone(), msg);
                    None
                }
                Ok(ParsedFrameRef::World) => Some(None),
                Ok(ParsedFrameRef::RobotLink { robot, link }) => match robot_ids.get(robot) {
                    None => {
                        issues.push(p.path.clone(), format!("unknown robot `{robot}`"));
                        None
                    }
                    Some(&r) if !robot_links[r].iter().any(|l| l == link) => {
                        issues.push(
                            p.path.clone(),
                            format!("robot `{robot}` has no link `{link}`"),
                        );
                        None
                    }
                    Some(_) => known.get(p.parent.as_str()).map(|&i| Some(i)),
                },
                Ok(ParsedFrameRef::Entity(id)) => {
                    if robot_ids.contains_key(id) {
                        issues.push(
                            p.path.clone(),
                            format!(
                                "`{id}` is a robot, not a frame; attach to one of its links \
                                 (`{id}/<link>`)"
                            ),
                        );
                        None
                    } else if let Some(&i) = known.get(id) {
                        Some(Some(i))
                    } else {
                        issues.push(p.path.clone(), format!("unknown parent frame `{id}`"));
                        None
                    }
                }
            };
            parent_of.push(resolved);
        }
        if !issues.is_empty() {
            return Err(issues.into_error());
        }
        let parent_of: Vec<Option<usize>> = parent_of.into_iter().map(Option::unwrap).collect();

        // Kahn-style passes that preserve declaration order.
        let mut placed: Vec<Option<usize>> = vec![None; pending.len()];
        let mut nodes = vec![Node {
            name: WORLD.to_owned(),
            kind: NodeKind::World,
        }];
        let mut order: Vec<usize> = Vec::with_capacity(pending.len());
        loop {
            let mut progressed = false;
            for i in 0..pending.len() {
                if placed[i].is_some() {
                    continue;
                }
                let ready = match parent_of[i] {
                    None => true,
                    Some(p) => placed[p].is_some(),
                };
                if ready {
                    placed[i] = Some(nodes.len());
                    nodes.push(Node {
                        name: pending[i].name.clone(),
                        kind: NodeKind::World, // placeholder, filled below
                    });
                    order.push(i);
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        for (i, p) in pending.iter().enumerate() {
            if placed[i].is_none() {
                issues.push(
                    p.path.clone(),
                    format!("frame `{}` is part of an attachment cycle", p.name),
                );
            }
        }
        if !issues.is_empty() {
            return Err(issues.into_error());
        }

        let node_of_parent = |i: usize| parent_of[i].map_or(0, |p| placed[p].unwrap());
        let mut robots: Vec<Option<RobotMount>> = vec![None; scene.robots.len()];
        for &i in &order {
            let node = placed[i].unwrap();
            let parent = node_of_parent(i);
            nodes[node].kind = match pending[i].what {
                PendingKind::Fixed(parent_se3_self) => NodeKind::Fixed {
                    parent,
                    parent_se3_self,
                },
                PendingKind::RobotLink { robot, link } => {
                    robots[robot].get_or_insert(RobotMount {
                        mount_parent: parent,
                        parent_se3_base: scene.robots[robot].parent_se3_self,
                    });
                    NodeKind::RobotLink { robot, link }
                }
            };
        }
        // A robot with an empty link list still needs a mount record.
        let robots = robots
            .into_iter()
            .zip(&scene.robots)
            .map(|(m, spec)| {
                m.unwrap_or(RobotMount {
                    mount_parent: 0,
                    parent_se3_base: spec.parent_se3_self,
                })
            })
            .collect();

        let index = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.name.clone(), i))
            .collect();
        Ok(Self {
            nodes,
            index,
            robots,
        })
    }

    /// Frame names in topological order (`"world"` first).
    pub fn names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.nodes.iter().map(|n| n.name.as_str())
    }

    /// Number of frames, including `"world"`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Always `false`: the graph contains at least `"world"`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Index of a frame in [`FrameGraph::names`] order.
    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    /// Compute `world_se3_frame` for every frame, in [`FrameGraph::names`]
    /// order.
    ///
    /// `base_se3_link(robot, link)` must return the pose of link `link` of
    /// robot `robot` (indices into `scene.robots` and into the
    /// `robot_links[robot]` list given to [`FrameGraph::build`]) relative to
    /// that robot's base link — the output of forward kinematics.
    pub fn resolve<F>(&self, mut base_se3_link: F) -> Vec<Isometry3<f64>>
    where
        F: FnMut(usize, usize) -> Isometry3<f64>,
    {
        let mut world: Vec<Isometry3<f64>> = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let pose = match &node.kind {
                NodeKind::World => Isometry3::identity(),
                NodeKind::Fixed {
                    parent,
                    parent_se3_self,
                } => world[*parent] * parent_se3_self,
                NodeKind::RobotLink { robot, link } => {
                    let mount = &self.robots[*robot];
                    world[mount.mount_parent] * mount.parent_se3_base * base_se3_link(*robot, *link)
                }
            };
            world.push(pose);
        }
        world
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_forms() {
        assert_eq!(FrameRef::world().parse().unwrap(), ParsedFrameRef::World);
        assert_eq!(
            FrameRef::from("ur5e/tool0").parse().unwrap(),
            ParsedFrameRef::RobotLink {
                robot: "ur5e",
                link: "tool0"
            }
        );
        assert_eq!(
            FrameRef::from("rig0").parse().unwrap(),
            ParsedFrameRef::Entity("rig0")
        );
    }

    #[test]
    fn rejects_malformed_refs() {
        for bad in ["", "/tool0", "ur5e/", "a b", "wörld"] {
            assert!(
                FrameRef::from(bad).parse().is_err(),
                "`{bad}` should not parse"
            );
        }
    }

    #[test]
    fn id_rules() {
        assert!(is_valid_id("cam_0-left"));
        assert!(!is_valid_id("world"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("a/b"));
    }
}
