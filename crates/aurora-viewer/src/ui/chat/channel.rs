//! The nearby-chat channel selector shared by the Conversations, Legacy
//! Viewer-style chat window and bottom-bar input fields. As the Legacy
//! Viewer's `ChatChannel` spinner (floater_fs_nearby_chat.xml, FSShowChatChannel),
//! its channel stays outside the text and is kept from one message to the
//! next; a `/n` typed before the text still wins (FSFloaterNearbyChat::sendChat).

use super::parse_channel;
use crate::theme::Palette;
use crate::ui::menu;
use egui::RichText;

/// Keep a 4-point gap and the 30-point selector inside the input row.
pub(super) const RESERVED_WIDTH: f32 = 34.0;

/// A click opens the channel's number; `channel` is the selector's own
/// value, shared by every nearby chat field. Returns whether its menu is open.
pub(super) fn show(ui: &mut egui::Ui, p: &Palette, channel: &mut i32, id: egui::Id) -> bool {
    show_impl(ui, p, channel, id, false)
}

/// A one-message selector for a conversation. Zero means an IM, never public chat.
pub(super) fn show_im(ui: &mut egui::Ui, p: &Palette, channel: &mut i32, id: egui::Id) -> bool {
    show_impl(ui, p, channel, id, true)
}

fn show_impl(ui: &mut egui::Ui, p: &Palette, channel: &mut i32, id: egui::Id, im: bool) -> bool {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 22.0), egui::Sense::hover());
    let resp = ui.interact(rect, id, egui::Sense::click());
    ui.painter().rect_filled(
        rect,
        ui.visuals().widgets.inactive.corner_radius,
        if resp.hovered() { p.field.gamma_multiply(1.3) } else { p.field },
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if im && *channel == 0 {
            "IM".to_owned()
        } else {
            format!("/{channel}")
        },
        egui::FontId::proportional(11.0),
        if *channel == 0 { p.muted } else { p.amber },
    );
    let resp = resp.on_hover_text(if im {
        aurora_i18n::tr!("custom-im-channel-commands-hint")
    } else {
        aurora_i18n::tr!("ui-chat-channel-tip")
    });
    menu::dropdown(&resp, p, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
        ui.set_min_width(170.0);
        ui.label(RichText::new(aurora_i18n::tr!("ui-chat-channel")).size(11.5).color(p.muted));
        let max = if im { i32::MAX - 1 } else { i32::MAX };
        ui.add(egui::DragValue::new(channel).range(i32::MIN..=max).speed(0.2));
        let reset = if im {
            aurora_i18n::tr!("custom-im-channel-reset")
        } else {
            aurora_i18n::tr!("ui-chat-channel-reset")
        };
        if *channel != 0 && menu::item(ui, p, "x", reset) {
            *channel = 0;
            ui.close();
        }
    })
}

/// The channel and message of a nearby line, read after the OOC wrap as the
/// Legacy Viewer does: a `/n` opening the text, else the selector's channel
/// when it is shown (`selector`), else 0.
pub(super) fn resolve(text: &str, selector: Option<i32>) -> (i32, String) {
    let (channel, message) = parse_channel(text);
    // parse_channel gives the text back unchanged when it has no `/n`
    match selector {
        Some(chosen) if message == text => (chosen, message),
        _ => (channel, message),
    }
}

/// Resolve only nonzero numbered channels in IMs. Explicit /0 stays an
/// ordinary IM (never public nearby chat), even with a selected channel.
/// The debug channel is also displayed publicly by viewers.
pub(super) fn resolve_im(text: &str, selector: i32) -> Option<(i32, String)> {
    let (channel, message) = parse_channel(text);
    if message != text {
        return (channel != 0 && channel != i32::MAX).then_some((channel, message));
    }
    (selector != 0 && selector != i32::MAX).then_some((selector, message))
}

#[cfg(test)]
mod tests {
    use super::{resolve, resolve_im};

    #[test]
    fn im_channels_route_only_nonzero_channels() {
        assert_eq!(resolve_im("/123 menu", 0), Some((123, "menu".into())));
        assert_eq!(resolve_im("/-500 status", 0), Some((-500, "status".into())));
        assert_eq!(resolve_im("/5", 0), Some((5, String::new())));
        assert_eq!(resolve_im("menu", 123), Some((123, "menu".into())));
        assert_eq!(resolve_im("/8 menu", 123), Some((8, "menu".into())));
        for text in ["hello", "/me waves", "/sysinfo", "/0 public", "/2147483648 test"] {
            assert_eq!(resolve_im(text, 0), None);
        }
        assert_eq!(resolve_im("/0 public", 123), None);
        assert_eq!(resolve_im("/2147483647 public", 123), None);
        assert_eq!(resolve_im("hello", i32::MAX), None);
    }

    #[test]
    fn the_selector_channel_applies_unless_the_text_names_one() {
        assert_eq!(resolve("hi", Some(5)), (5, "hi".into()));
        assert_eq!(resolve("/7 hi", Some(5)), (7, "hi".into()));
        // "/0" typed is a channel set, as stripChannelNumber's is_set
        assert_eq!(resolve("/0 hi", Some(5)), (0, "hi".into()));
        assert_eq!(resolve("/me waves", Some(5)), (5, "/me waves".into()));
        // hidden selector: the text alone decides
        assert_eq!(resolve("hi", None), (0, "hi".into()));
        // Alt+Enter wraps first, so the selector's channel carries the OOC line
        assert_eq!(resolve(&super::super::ooc("brb"), Some(5)), (5, "(( brb ))".into()));
    }
}
