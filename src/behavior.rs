//! Autonomous goal-oriented behavior system.
//!
//! Every tick the agent selects a behavior goal from its observations
//! (health, position, nearby entities) and converts the chosen goal into a
//! movement steering direction. Goals are prioritized so urgent needs (avoid
//! danger) override routine ones (gather, explore). A cooldown system
//! prevents goal thrashing. The Explore goal reuses the patrol/wander
//! steering already computed by the tick loop, so behavior integrates with
//! patrol instead of replacing it.
//!
//! This module is pure logic over observation data: it does not touch the
//! network, which makes it unit-testable without a Veloren server.

use crate::HealthObs;
use std::cmp::Ordering;
use vek::Vec2;

/// Health ratio (current / maximum) below which danger avoidance outranks
/// every other goal.
const HEALTH_LOW_RATIO: f32 = 0.30;

/// Ticks the agent keeps a goal before re-selecting. Prevents flapping
/// between goals every tick (2 s at 60 ticks/sec).
const GOAL_COOLDOWN_TICKS: u64 = 120;

/// Ticks between gather actions toward the same set of nearby entities.
const GATHER_COOLDOWN_TICKS: u64 = 60;

/// Radius (world units) within which an entity counts as "nearby" for
/// gather / avoid decisions.
pub(crate) const NEARBY_ENTITY_RADIUS: f32 = 8.0;

/// Distance (world units) at which the agent counts as arrived at a gather
/// target and stops moving toward it.
const GATHER_ARRIVE_DIST: f32 = 1.0;

/// Behavior goal the agent is currently pursuing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Goal {
    /// Nothing urgent: follow the patrol / wander steering.
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
    /// Smoothed patrol/wander steering (already obstacle-aware) produced by
    /// the tick loop; used as the movement for the Explore goal.
    pub patrol_steering: Vec2<f32>,
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
}

impl Default for BehaviorSystem {
    fn default() -> Self {
        Self {
            goal: Goal::Explore,
            goal_cooldown: 0,
            gather_cooldown: 0,
        }
    }
}

impl BehaviorSystem {
    /// Advance cooldown counters by one tick. Call once per agent tick.
    pub fn tick(&mut self) {
        self.goal_cooldown = self.goal_cooldown.saturating_sub(1);
        self.gather_cooldown = self.gather_cooldown.saturating_sub(1);
    }

    /// Select the active goal from observations, respecting the goal
    /// cooldown. While `goal_cooldown` is nonzero the current goal is kept.
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
            Goal::Explore => inputs.patrol_steering,
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
        if to_entity.magnitude() < GATHER_ARRIVE_DIST {
            Vec2::zero() // arrived at the entity
        } else {
            normalize(to_entity)
        }
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

    fn inputs(
        health: Option<HealthObs>,
        position: Option<[f32; 3]>,
        entities: Vec<[f32; 3]>,
        patrol_steering: Vec2<f32>,
    ) -> BehaviorInputs {
        BehaviorInputs { health, position, nearby_entities: entities, patrol_steering }
    }

    #[test]
    fn low_health_selects_avoid_danger() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs(Some(health(20.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![], Vec2::zero());
        assert_eq!(b.select_goal(&inputs), Goal::AvoidDanger);
    }

    #[test]
    fn dead_selects_avoid_danger() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs(Some(health(0.0, 100.0, true)), Some([0.0, 0.0, 0.0]), vec![], Vec2::zero());
        assert_eq!(b.select_goal(&inputs), Goal::AvoidDanger);
    }

    #[test]
    fn nearby_entities_selects_gather() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs(
            Some(health(100.0, 100.0, false)),
            Some([0.0, 0.0, 0.0]),
            vec![[5.0, 0.0, 0.0]],
            Vec2::zero(),
        );
        assert_eq!(b.select_goal(&inputs), Goal::Gather);
    }

    #[test]
    fn nothing_urgent_selects_explore() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs(Some(health(100.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![], Vec2::zero());
        assert_eq!(b.select_goal(&inputs), Goal::Explore);
    }

    #[test]
    fn goal_cooldown_keeps_current_goal() {
        let mut b = BehaviorSystem::default();
        let healthy = inputs(Some(health(100.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![], Vec2::zero());
        let dangerous = inputs(Some(health(20.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![], Vec2::zero());

        assert_eq!(b.select_goal(&healthy), Goal::Explore);
        // Cooldown active: dangerous inputs must not change the goal.
        assert_eq!(b.select_goal(&dangerous), Goal::Explore);
        assert_eq!(b.goal, Goal::Explore);
    }

    #[test]
    fn goal_cooldown_expires_after_ticks() {
        let mut b = BehaviorSystem::default();
        let healthy = inputs(Some(health(100.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![], Vec2::zero());
        let dangerous = inputs(Some(health(20.0, 100.0, false)), Some([0.0, 0.0, 0.0]), vec![], Vec2::zero());

        b.select_goal(&healthy); // Explore, cooldown = GOAL_COOLDOWN_TICKS
        for _ in 0..GOAL_COOLDOWN_TICKS {
            b.tick();
        }
        assert_eq!(b.select_goal(&dangerous), Goal::AvoidDanger);
    }

    #[test]
    fn action_priority_avoid_danger_highest() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs(
            Some(health(20.0, 100.0, false)),
            Some([0.0, 0.0, 0.0]),
            vec![[5.0, 0.0, 0.0]],
            Vec2::zero(),
        );
        let action = b.chosen_action(&inputs);
        assert_eq!(action.goal, Goal::AvoidDanger);
        assert_eq!(action.priority, Goal::AvoidDanger.priority());
        assert!(action.priority > Goal::Gather.priority());
        assert!(action.priority > Goal::Explore.priority());
    }

    #[test]
    fn gather_action_moves_toward_nearest_entity() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs(
            Some(health(100.0, 100.0, false)),
            Some([0.0, 0.0, 0.0]),
            vec![[10.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            Vec2::zero(),
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
        let inputs = inputs(
            Some(health(20.0, 100.0, false)),
            Some([0.0, 0.0, 0.0]),
            vec![[5.0, 0.0, 0.0]],
            Vec2::zero(),
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
        let inputs = inputs(
            Some(health(100.0, 100.0, false)),
            Some([3.0, 0.0, 0.0]),
            vec![[3.0, 0.0, 0.0]],
            Vec2::zero(),
        );
        let action = b.action_for(Goal::Gather, &inputs);
        assert_eq!(action.move_dir, Vec2::zero());
    }

    #[test]
    fn explore_uses_patrol_steering() {
        let mut b = BehaviorSystem::default();
        let inputs = inputs(
            Some(health(100.0, 100.0, false)),
            Some([0.0, 0.0, 0.0]),
            vec![],
            Vec2::new(0.0, 1.0),
        );
        let action = b.action_for(Goal::Explore, &inputs);
        assert_eq!(action.goal, Goal::Explore);
        assert_eq!(action.move_dir, Vec2::new(0.0, 1.0));
    }

    #[test]
    fn nearest_entity_picks_closest() {
        let cur = Vec2::new(0.0, 0.0);
        let entities = vec![[10.0, 0.0, 0.0], [1.0, 0.0, 0.0], [5.0, 0.0, 0.0]];
        let nearest = nearest_entity(cur, &entities).unwrap();
        assert_eq!(nearest, [1.0, 0.0, 0.0]);
    }
}
