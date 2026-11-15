//! Explicit body facing, separate from camera look-at and travel.
use super::*;
use crate::world::facing;

impl App {
    pub(super) fn face_avatar(&mut self, id: uuid::Uuid, continuous: bool) {
        let yaw = self.in_world().then(|| facing::target_yaw(&self.world, id)).flatten();
        let Some(yaw) = yaw.filter(|_| !self.camera.mouselook()) else {
            self.world
                .system_notice(aurora_i18n::tr!("facing-unavailable"), crate::world::notice_topics::Topic::Warning);
            return;
        };
        // A pilot must not undo the turn on its next frame. Follow remains
        // resumable; a point-travel or recovery is cancelled by this action.
        if self.follow_recovery.is_some() || self.autopilot.as_ref().is_some_and(|ap| ap.leader.is_none()) {
            self.stop_autopilot(true);
        } else if let Some(ap) = &mut self.autopilot
            && ap.controls_active()
        {
            ap.hold_follow(crate::autopilot::FollowStatus::Paused, self.world.agent.position);
            self.autopilot_flags = aurora_net::control::STOP;
            self.autopilot_stop_pending = true;
            self.sync_run();
        }
        self.selection_turn = None;
        self.world.facing.stop();
        if continuous {
            self.world.facing.start(id);
            let mut open = false;
            ui::widgets::open_floater(&self.egui_ctx, "facing", &mut open);
        }
        self.world.agent.yaw = yaw;
    }

    pub(super) fn face_nearest(&mut self, continuous: bool) {
        if !self.in_world() || !facing::can_turn(&self.world) || self.camera.mouselook() {
            self.world
                .system_notice(aurora_i18n::tr!("facing-unavailable"), crate::world::notice_topics::Topic::Warning);
            return;
        }
        if let Some(id) = facing::nearest_avatar(&self.world) {
            self.face_avatar(id, continuous);
        } else {
            self.world
                .system_notice(aurora_i18n::tr!("facing-no-nearby"), crate::world::notice_topics::Topic::Warning);
        }
    }

    pub(super) fn update_facing(&mut self) {
        if self.world.facing.target.is_none() {
            return;
        }
        if !self.in_world()
            || self.camera.mouselook()
            || self.mouse_mode == MouseMode::Steer
            || self.input.turn_left
            || self.input.turn_right
            || self.input.joystick_yaw.abs() > 0.001
        {
            self.world.facing.stop();
            return;
        }
        facing::tick(&mut self.world);
    }

    pub(super) fn facing_window(&mut self, ctx: &egui::Context, p: &crate::theme::Palette) {
        let Some(id) = self.world.facing.target else { return };
        let name = self
            .world
            .person_name(&id)
            .unwrap_or_else(|| aurora_i18n::tr!("facing-title").into());
        if ui::facing::window(ctx, p, &name, self.world.facing.status) {
            self.world.facing.stop();
        }
    }
}
