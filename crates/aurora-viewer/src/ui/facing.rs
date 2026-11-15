//! Controller for the explicitly locked facing target. Closing stops it.
use super::widgets::{self, ActionKind, Floater};
use crate::{theme::Palette, world::facing::Status};

pub fn window(ctx: &egui::Context, p: &Palette, name: &str, status: Status) -> bool {
    let mut open = true;
    let mut stop = false;
    Floater::new(
        "facing",
        aurora_i18n::tr!("facing-title"),
        egui::pos2(40.0, 320.0),
        egui::vec2(360.0, 180.0),
    )
    .fixed()
    .show(ctx, p, &mut open, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.label(egui::RichText::new(name).strong());
        ui.label(match status {
            Status::Facing => aurora_i18n::tr!("facing-active"),
            Status::Waiting => aurora_i18n::tr!("facing-waiting"),
            Status::Paused => aurora_i18n::tr!("facing-paused"),
            Status::Stopped => aurora_i18n::tr!("facing-stop"),
        });
        stop = widgets::action_button(
            ui,
            p,
            "player-stop-filled",
            aurora_i18n::tr!("facing-stop"),
            ActionKind::Flat,
            egui::vec2(ui.available_width(), 28.0),
        )
        .clicked();
    });
    stop || !open
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::elide_tests::Frames;

    #[test]
    fn facing_controller_displays_waiting_and_stops_through_its_real_button() {
        let mut frames = Frames::new();
        let p = crate::theme::Theme::default().palette();
        let mut stop = false;
        let mut show = |ui: &mut egui::Ui| {
            crate::ui::close_key::begin(ui.ctx());
            stop = window(ui.ctx(), &p, "Selected resident", Status::Waiting);
        };
        frames.run(vec![], &mut show);
        let texts = frames.run(vec![], &mut show);
        assert!(texts.iter().any(|t| t.source == aurora_i18n::tr!("facing-waiting")));
        let pos = texts
            .iter()
            .find(|t| t.source == aurora_i18n::tr!("facing-stop"))
            .unwrap()
            .rect
            .center();
        for pressed in [true, false] {
            frames.run(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                &mut show,
            );
        }
        assert!(stop);
    }
}
