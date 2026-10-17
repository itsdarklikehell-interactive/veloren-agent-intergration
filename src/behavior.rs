//! Autonomous goal-oriented behavior system.
//!
//! Every tick the agent selects a behavior goal from its observations
//! (health, position, nearby entities) and converts the chosen goal into a
//! movement steering direction. Goals are prioritized so urgent needs (avoid
//! danger) override routine ones (gather, explore). A cooldown system prevents
//! goal thrashing, and patrol waypoints are fed into the Explore goal so
//! patrol and autonomous behavior share one steering source.
//!
//! This module is pure logic over observation data: it does not touch the
//! network, which makes it unit-testable without a Veloren server.

use crate::{HealthObs, PATROL_REACH_THRESHOLD};
use std::cmp::Ordering;
use std::f32::consts::PI;
use vek::Vec2;

/// Health ratio (current / maximum) below which danger avoidance takes
/// priority over every other goal.
pub(crate) const HEALTH_LOW_RATIO: f32 = 0.30;

/// Ticks the agent sticks with its current goal before re-selecting. Prevents
/// flapping between goals every tick (2 s at 60 ticks/sec).
pub(crate) const GOAL_COOLDOWN_TICKS: u64 = 120;

/// Ticks between gather actions toward the same set of nearby entities.
pub(crate) const GATHER_COOLDOWN_TICKS: u64 = 60;

/// Radius (world units) within which an entity counts as "nearby" for gather
/// / avoid decisions.
pub(crate) const NEARBY_ENTITY_RADIUS: f32 = 8.0;

/// Radius (world units) at which an explore / patrol target counts as reached.
pub(crate) const EXPLORE_REACH_THRESHOLD: f32 = PATROL_REACH_THRESHOLD;

/// Distance (world units) from the agent at which a fresh wander target is
/// placed when exploring.
pub(crate) const WANDER_RADIUS: f32 = 12.0;

/// Behavior goal the agent is currently pursuing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Goal {
    /// Nothing urgent: wander the world or follow the patrol waypoint.
    #[default]
    Explore,
    /// Move toward nearby entities (resources, items) to gather them.
    Gather,
    /// Move away from threats because health is low (or the agent is dead).
    AvoidDanger,
}

impl Goal {
    /// Priority used by action selection; higher wins.
    pub fn priority(self) -> u8 {
        match self {
            Goal::AvoidDanger => 3,
            Goal::Gather => 2,
            Goal::Explore => 1,
        }
    }
}

/// Observation inputs fed to goal selection.
#[derive(Debug, Clone, Default)]
pub struct BehaviorInputs {
    pub health: Option<HealthObs>,
    pub position: Option<[f32; 3]>,
    pub nearby_entities: Vec<[f32; 3]>,
}

/// A candidate movement action produced by a goal.
#[derive(Debug, Clone)]
pub struct BehaviorAction {
    pub goal: Goal,
    pub move_dir: Vec2<f32>,
    pub priority: u8,
}

/// State of the autonomous behavior system.
pub struct BehaviorSystem {
    goal: Goal,
    goal_cooldown: u64,
    gather_cooldown: u64,
    /// Patrol waypoint the Explore goal should steer toward (`None` = wander).
    explore_target: Option<[f32; 2]>,
    /// Computed explore destination for pure wandering.
    wander_target: Option<[f32; 2]>,
    wander_angle: f32,
}

impl Default for BehaviorSystem {
    fn default() -> Self {
        Self {
            goal: Goal::Explore,
            goal_cooldown: 0,
            gather_cooldown: 0,
            explore_target: None,
            wander_target: None,
            wander_angle: 0.0,
        }
    }
}

impl BehaviorSystem {
    /// Advance cooldown counters by one tick. Call once per agent tick.
    pub fn tick(&mut self) {
        self.goal_cooldown = self.goal_cooldown.saturating_sub(1);
        self.gather_cooldown = self.gather_cooldown.saturating_sub(1);
    }

    /// Set the patrol waypoint the Explore goal should steer toward. `None`
    /// clears the patrol target so pure wandering resumes.
    pub fn set_patrol_target(&mut self, target: Option<[f32; 2]>) {
        self.explore_target = target;
    }

    /// Select the active goal from observations, respecting the goal cooldown.
    /// While `goal_cooldown` is nonzero the current goal is kept.
    pub fn select_goal(&mut self, inputs: &BehaviorInputs) -> Goal {
        if self.goal_cooldown > 0 {
            return self.goal;
        }

        let health_ratio = inputs
            .health
            .map(|h| {
                if h.maximum > 0.0 {
                    h.current / h.maximum
                } else {
                    1.0
                }
            })
            .unwrap_or(1.0);
        let is_dead = inputs.health.map(|h| h.is_dead).unwrap_or(false);
        let has_entities = !inputs.nearby_entities.is_empty();

        let new_goal = if is_dead || health_ratio < HEALTH_LOW_RATIO {
            Goal::AvoidDanger
        } else if has_entities && self.gather_cooldown == 0 {
            Goal::Gather
        } else {
            Goal::Explore
        };

        if new_goal != self.goal {
            self.goal = new_goal;
            self.goal_cooldown = GOAL_COOLDOWN_TICKS;
        }
        self.goal
    }

    /// Convert a goal into a movement steering direction.
    pub fn action_for(&mut self, goal: Goal, inputs: &BehaviorInputs) -> BehaviorAction {
        let priority = goal.priority();
        let move_dir = match goal {
            Goal::AvoidDanger => self.avoid_action(inputs),
            Goal::Gather => {
                if self.gather_cooldown > 0 {
                    Vec2::zero()
                } else {
                    self.gather_action(inputs)
                }
            }
            Goal::Explore => self.explore_action(inputs),
        };
        BehaviorAction {
            goal,
            move_dir,
            priority,
        }
    }

    /// Run `tick`, `select_goal` and `action_for` in one step.
    pub fn chosen_action(&mut self, inputs: &BehaviorInputs) -> BehaviorAction {
        self.tick();
        let goal = self.select_goal(inputs);
        self.action_for(goal, inputs)
    }

    /// Steer away from the nearest nearby entity (flee). Idles when there is
    /// no entity or no position to flee from.
    fn avoid_action(&self, inputs: &BehaviorInputs) -> Vec2<f32> {
        let Some(cur) = inputs.position.map(|p| Vec2::new(p[0], p[1])) else {
            return Vec2::zero();
        };
        let Some(nearest) = nearest_entity(cur, &inputs.nearby_entities) else {
            return Vec2::zero();
        };
        let away = Vec2::new(cur[0] - nearest[0], cur[1] - nearest[1]);
        normalize(away)
    }

    /// Steer toward the nearest nearby entity (gather). Idles when arrived.
    fn gather_action(&self, inputs: &BehaviorInputs) -> Vec2<f32> {
        let Some(cur) = inputs.position.map(|p| Vec2::new(p[0], p[1])) else {
            return Vec2::zero();
        };
        let Some(nearest) = nearest_entity(cur, &inputs.nearby_entities) else {
            return Vec2::zero();
        };
        let to_entity = Vec2::new(nearest[0] - cur[0], nearest[1] - cur[1]);
        if to_entity.magnitude() < 1.0 {
            Vec2::zero() // arrived at the entity
        } else {
            normalize(to_entity)
        }
    }

    /// Steer toward the patrol waypoint when patrolling, otherwise toward a
    /// wandering destination that is refreshed each time it is reached.
    fn explore_action(&mut self, inputs: &BehaviorInputs) -> Vec2<f32> {
        let Some(cur_pos) = inputs.position else {
            return Vec2::zero();
        };
        let cur = Vec2::new(cur_pos[0], cur_pos[1]);

        let target = if let Some(wp) = self.explore_target {
            Vec2::new(wp[0], wp[1])
        } else {
            self.wander_target(cur)
        };

        let to_target = target - cur;
        let dist = to_target.magnitude();
        if dist < EXPLORE_REACH_THRESHOLD {
            // Reached the destination: pick a fresh wander target next tick.
            if self.explore_target.is_none() {
                self.wander_target = None;
            }
            Vec2::zero()
        } else {
            normalize(to_target)
        }
    }

    /// Pick (or reuse) a wander destination at `WANDER_RADIUS` from `cur`.
    fn wander_target(&mut self, cur: Vec2<f32>) -> Vec2<f32> {
        if let Some(wt) = self.wander_target {
            let wt2 = Vec2::new(wt[0], wt[1]);
            if (wt2 - cur).magnitude() < EXPLORE_REACH_THRESHOLD {
                self.wander_target = None;
            } else {
                return wt2;
            }
        }
        self.wander_angle = (self.wander_angle + 0.5).rem_euclid(2.0 * PI);
        let offset = Vec2::new(self.wander_angle.cos(), self.wander_angle.sin()) * WANDER_RADIUS;
        let target = cur + offset;
        self.wander_target = Some([target.x, target.y]);
        target
    }
}

/// Position of the nearest entity to `cur`, using squared 2-D distance.
fn nearest_entity(cur: Vec2<f32>, entities: &[[f32; 3]]) -> Option<[f32; 3]> {
    entities
        .iter()
        .min_by(|a, b| {
            let da = dist2(cur, Vec2::new(a[0], a[1]));
            let db = dist2(cur, Vec2::new(b[0], b[1]));
            da.partial_cmp(&db).unwrap_or(Ordering::Equal)
        })
        .copied()
}

fn dist2(a: Vec2<f32>, b: Vec2<f32>) -> f32 {
    let d = a - b;
    d.x * d.x + d.y * d.y
}

fn normalize(v: Vec2<f32>) -> Vec2<f32> {
    let mag = v.magnitude();
    if mag > 0.001 {
        v * (1.0 / mag)
    } else {
        Vec2::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn health(current: f32, maximum: f32, is_dead: bool) -> HealthObs {
        HealthObs { current, maximum, is_dead }
    }

    fn inputs_with(health: Option<HealthObs>, position: Option<[f32; 3]>, entities: Vec<[f32; 3]>) -> BehaviorInputs {
        BehaviorInputs { health, position, nearby_entities: entities }
    }

    #[test]
    fn low_health_selects_avoid_danger() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs_with(Some(health(20.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![]);
        assert_eq!(b.select_goal(&inputs), Goal::AvoidDanger);
    }

    #[test]
    fn dead_selects_avoid_danger() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs_with(Some(health(0.0, 100.0, true)), Some([0.0, 0.0, 0.0]), vec![]);
        assert_eq!(b.select_goal(&inputs), Goal::AvoidDanger);
    }

    #[test]
    fn nearby_entities_selects_gather() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs_with(Some(health(100.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![[5.0, 0.0, 0.0]]);
        assert_eq!(b.select_goal(&inputs), Goal::Gather);
    }

    #[test]
    fn nothing_urgent_selects_explore() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs_with(Some(health(100.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![]);
        assert_eq!(b.select_goal(&inputs), Goal::Explore);
    }

    #[test]
    fn goal_cooldown_keeps_current_goal() {
        let mut b = BehaviorSystem::default();
        let healthy = inputs_with(Some(health(100.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![]);
        let dangerous = inputs_with(Some(health(20.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![]);

        assert_eq!(b.select_goal(&healthy), Goal::Explore);
        // Cooldown is active: switching to dangerous inputs must not change the goal.
        assert_eq!(b.select_goal(&dangerous), Goal::Explore);
        assert_eq!(b.goal, Goal::Explore);
    }

    #[test]
    fn goal_cooldown_expires_after_ticks() {
        let mut b = BehaviorSystem::default();
        let healthy = inputs_with(Some(health(100.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![]);
        let dangerous = inputs_with(Some(health(20.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![]);

        b.select_goal(&healthy); // Explore, cooldown = GOAL_COOLDOWN_TICKS
        for _ in 0..GOAL_COOLDOWN_TICKS {
            b.tick();
        }
        assert_eq!(b.select_goal(&dangerous), Goal::AvoidDanger);
    }

    #[test]
    fn action_priority_avoid_danger_highest() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs_with(Some(health(20.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![[5.0, 0.0, 0.0]]);
        let action = b.chosen_action(&inputs);
        assert_eq!(action.goal, Goal::AvoidDanger);
        assert_eq!(action.priority, Goal::AvoidDanger.priority());
        assert!(action.priority > Goal::Gather.priority());
        assert!(action.priority > Goal::Explore.priority());
    }

    #[test]
    fn gather_action_moves_toward_nearest_entity() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs_with(
            Some(health(100.0, 100.0, false)),
            Some([0.0, 0.0, 0.0]),
            vec![[10.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        );
        let action = b.action_for(Goal::Gather, &inputs);
        assert_eq!(action.goal, Goal::Gather);
        // Nearest entity is [1,0,0]; direction should be +X.
        assert!((action.move_dir.x - 1.0).abs() < 0.001);
        assert!(action.move_dir.y.abs() < 0.001);
    }

    #[test]
    fn avoid_action_moves_away_from_nearest_entity() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs_with(
            Some(health(20.0, 100.0, false)),
            Some([0.0, 0.0, 0.0]),
            vec![[5.0, 0.0, 0.0]],
        );
        let action = b.action_for(Goal::AvoidDanger, &inputs);
        assert_eq!(action.goal, Goal::AvoidDanger);
        // Entity is at +X; fleeing direction should be -X.
        assert!((action.move_dir.x - (-1.0)).abs() < 0.001);
        assert!(action.move_dir.y.abs() < 0.001);
    }

    #[test]
    fn gather_idles_when_at_entity() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs_with(
            Some(health(100.0, 100.0, false)),
            Some([3.0, 0.0, 0.0]),
            vec![[3.0, 0.0, 0.0]],
        );
        let action = b.action_for(Goal::Gather, &inputs);
        assert_eq!(action.move_dir, Vec2::zero());
    }

    #[test]
    fn explore_moves_toward_patrol_target() {
        let mut b = BehaviorSystem::default();
        b.set_patrol_target(Some([10.0, 0.0]));
        let inputs = inputs_with(Some(health(100.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![]);
        let action = b.action_for(Goal::Explore, &inputs);
        assert!((action.move_dir.x - 1.0).abs() < 0.001);
        assert!(action.move_dir.y.abs() < 0.001);
    }

    #[test]
    fn explore_idles_at_patrol_target() {
        let mut b = BehaviorSystem::default();
        b.set_patrol_target(Some([0.0, 0.0]));
        let inputs = inputs_with(Some(health(100.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![]);
        let action = b.action_for(Goal::Explore, &inputs);
        assert_eq!(action.move_dir, Vec2::zero());
    }

    #[test]
    fn nearest_entity_picks_closest() {
        let cur = Vec2::new(0.0, 0.0);
        let entities = vec![[10.0, 0.0, 0.0], [1.0, 0.0, 0.0], [5.0, 0.0, 0.0]];
        let nearest = nearest_entity(cur, &entities).unwrap();
        assert_eq!(nearest, [1.0, 0.0, 0.0]);
    }
}
