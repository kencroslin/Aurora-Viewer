//! One-shot body orientation and an explicitly locked continuous target.
//! Uses precise loaded avatar positions, never coarse map or friend locations.
use super::World;
use glam::Vec3;
use uuid::Uuid;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Status {
    #[default]
    Stopped,
    Facing,
    Waiting,
    Paused,
}

#[derive(Default)]
pub struct State {
    pub target: Option<Uuid>,
    pub status: Status,
}

impl State {
    pub fn start(&mut self, target: Uuid) {
        self.target = Some(target);
        self.status = Status::Facing;
    }

    pub fn stop(&mut self) {
        self.target = None;
        self.status = Status::Stopped;
    }
}

pub fn can_turn(world: &World) -> bool {
    world.movement_complete && !world.teleporting && !world.frozen && !world.agent.is_sitting() && world.agent.position.is_finite()
}

pub fn yaw_to(from: Vec3, to: Vec3) -> Option<f32> {
    if !from.is_finite() || !to.is_finite() {
        return None;
    }
    let delta = (to - from).truncate();
    if !delta.is_finite() {
        return None;
    }
    (delta.length_squared() > 0.0001).then(|| delta.y.atan2(delta.x))
}

pub fn target_yaw(world: &World, id: Uuid) -> Option<f32> {
    if !can_turn(world) {
        return None;
    }
    yaw_to(world.agent.position, crate::autopilot::avatar_position(world, id)?)
}

/// Stable ties and full 3D proximity avoid selecting someone on another floor
/// merely because their horizontal position coincides with ours.
fn nearest_from(from: Vec3, own: Uuid, candidates: impl Iterator<Item = (Uuid, Vec3)>) -> Option<Uuid> {
    candidates
        .filter_map(|(id, position)| {
            if id.is_nil() || id == own || yaw_to(from, position).is_none() {
                return None;
            }
            let distance = from.distance_squared(position);
            distance.is_finite().then_some((id, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|(id, _)| id)
}

pub fn nearest_avatar(world: &World) -> Option<Uuid> {
    if !can_turn(world) {
        return None;
    }
    nearest_from(
        world.agent.position,
        world.agent_id,
        world.objects.avatars().filter_map(|(_, avatar)| {
            let id = avatar.full_id;
            if world.is_avatar_blocked(&id) {
                return None;
            }
            Some((id, crate::autopilot::avatar_position(world, id)?))
        }),
    )
}

/// A lost target is retained without switching to a different nearby person.
/// Sitting/freeze pauses orientation without standing or changing position.
pub fn tick(world: &mut World) {
    let Some(id) = world.facing.target else { return };
    if !can_turn(world) {
        world.facing.status = Status::Paused;
    } else if let Some(yaw) = target_yaw(world, id) {
        world.agent.yaw = yaw;
        world.facing.status = Status::Facing;
    } else {
        world.facing.status = Status::Waiting;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{demo, scene::avatar::AvatarLibrary};
    use aurora_net::NetEvent;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn fixture() -> (World, Uuid, Uuid) {
        let handle = aurora_net::origin_to_handle(256000, 256000);
        let mut world = World::new(std::sync::Arc::new(AvatarLibrary::load()));
        world.agent_id = demo::DEMO_AGENT;
        world.main_region = Some(handle);
        let a = Uuid::from_u128(800);
        let b = Uuid::from_u128(801);
        let mut first = demo::action_avatar(true);
        first.full_id = a;
        first.local_id = 800;
        first.parent_id = 0;
        first.position = Vec3::new(10.0, 15.0, 2.0);
        first.velocity = Vec3::ZERO;
        let mut second = first.clone();
        second.full_id = b;
        second.local_id = 801;
        second.position = Vec3::new(20.0, 10.0, 2.0);
        world.apply(NetEvent::ObjectUpdates {
            handle,
            objects: vec![first, second],
        });
        world.agent.position = Vec3::new(10.0, 10.0, 2.0);
        world.movement_complete = true;
        (world, a, b)
    }

    #[test]
    fn facing_headings_use_body_yaw_and_reject_invalid_or_vertical_targets() {
        for (point, yaw) in [(Vec3::X, 0.0), (Vec3::Y, FRAC_PI_2), (Vec3::NEG_X, PI), (Vec3::NEG_Y, -FRAC_PI_2)] {
            assert!((yaw_to(Vec3::ZERO, point).unwrap() - yaw).abs() < 0.0001);
        }
        for point in [Vec3::ZERO, Vec3::Z, Vec3::NAN, Vec3::splat(f32::INFINITY)] {
            assert_eq!(yaw_to(Vec3::ZERO, point), None);
        }
        assert_eq!(yaw_to(Vec3::splat(f32::MAX), Vec3::splat(-f32::MAX)), None);
    }

    #[test]
    fn facing_nearest_is_deterministic_excludes_self_and_uses_three_dimensional_distance() {
        let own = Uuid::from_u128(1);
        let a = Uuid::from_u128(2);
        let b = Uuid::from_u128(3);
        let candidates = [
            (own, Vec3::X),
            (Uuid::nil(), Vec3::Y),
            (b, Vec3::NEG_X * 5.0),
            (a, Vec3::X * 5.0),
            (Uuid::from_u128(4), Vec3::new(1.0, 0.0, 50.0)),
        ];
        assert_eq!(nearest_from(Vec3::ZERO, own, candidates.into_iter()), Some(a));
        assert_eq!(nearest_from(Vec3::ZERO, own, candidates.into_iter().rev()), Some(a));
        assert_eq!(nearest_from(Vec3::ZERO, own, [(a, Vec3::Z)].into_iter()), None);
    }

    #[test]
    fn facing_continuous_locks_identity_tracks_motion_and_never_moves_the_avatar() {
        let (mut world, a, b) = fixture();
        assert_eq!(nearest_avatar(&world), Some(a));
        let position = world.agent.position;
        world.facing.start(a);
        tick(&mut world);
        assert!((world.agent.yaw - FRAC_PI_2).abs() < 0.001);
        let index = world.objects.index_of_uuid(&a).unwrap();
        world.objects.get_mut(index).unwrap().position = Vec3::new(30.0, 10.0, 2.0);
        let index = world.objects.index_of_uuid(&b).unwrap();
        world.objects.get_mut(index).unwrap().position = Vec3::new(10.0, 11.0, 2.0);
        assert_eq!(nearest_avatar(&world), Some(b));
        tick(&mut world);
        assert_eq!(world.facing.target, Some(a));
        assert!(world.agent.yaw.abs() < 0.001);
        assert_eq!(world.agent.position, position);
        assert_eq!(world.facing.status, Status::Facing);
        world.facing.stop();
        world.agent.yaw = 1.25;
        tick(&mut world);
        assert_eq!(world.agent.yaw, 1.25);
    }

    #[test]
    fn facing_waits_for_same_target_and_pauses_without_unsitting_or_bypassing_freeze() {
        let (mut world, a, _) = fixture();
        world.facing.start(a);
        let index = world.objects.index_of_uuid(&a).unwrap();
        world.objects.remove_idx(index);
        world.coarse.insert(world.main_region.unwrap(), vec![(a, Vec3::new(1.0, 0.0, 0.0))]);
        world.agent.yaw = 0.7;
        tick(&mut world);
        assert_eq!(world.facing.status, Status::Waiting);
        assert_eq!(world.facing.target, Some(a));
        assert_eq!(world.agent.yaw, 0.7, "coarse positions must not drive facing");
        world.agent.seated = true;
        tick(&mut world);
        assert_eq!(world.facing.status, Status::Paused);
        assert!(world.agent.seated);
        world.agent.seated = false;
        world.frozen = true;
        tick(&mut world);
        assert_eq!(world.facing.status, Status::Paused);
        assert_eq!(world.agent.yaw, 0.7);
        world.frozen = false;
        let mut target = demo::action_avatar(true);
        target.full_id = a;
        target.local_id = 800;
        target.parent_id = 0;
        target.position = Vec3::new(10.0, 15.0, 2.0);
        target.velocity = Vec3::ZERO;
        world.apply(NetEvent::ObjectUpdates {
            handle: world.main_region.unwrap(),
            objects: vec![target],
        });
        tick(&mut world);
        assert_eq!(world.facing.status, Status::Facing);
        assert!((world.agent.yaw - FRAC_PI_2).abs() < 0.001);
    }
}
