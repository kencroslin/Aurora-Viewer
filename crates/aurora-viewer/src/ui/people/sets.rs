//! People â€º Contact sets in Aurora's look (the People mock-up v2): the
//! contact sets of Conversations â€º Contacts (FSPanelContactSets,
//! fspanelcontactsets.cpp, panel_fs_contacts_sets.xml, LGGContactSets) with
//! the sets as colored pills, "New", and a gear for the set's color, name
//! and deletion (FSFloaterContactSetConfiguration), the pseudo-sets (all,
//! none, with a nickname, non-friends). The members
//! are rich rows; under the pointer a row offers to move the resident to
//! another set, to take them out of this one (confirmed) and its menu.
//! Non-friends stay in their sets. Nicknames are in the sheet.

use super::rows::{self, RICH_GAP, RICH_H};
use super::{Ctx, PeopleUi};
use crate::ui::context::{self, AvatarList};
use crate::ui::{menu, widgets};
use crate::world::contact_sets::{self, ContactSets, DN_REMOVED};
use egui::{Rect, Sense, Vec2};
use uuid::Uuid;

/// The contact sets tab: the list, and the sheet of the person clicked.
pub(super) fn tab(ui: &mut egui::Ui, c: &mut Ctx, data: &super::RadarData, st: &mut PeopleUi) {
    super::split(ui, c, data, st, "people-sets-list", list, super::person_sheet);
}

/// The set shown: the one chosen while it exists, else the first set, else
/// all of them (FSPanelContactSets::refreshSetList).
fn choice(c: &mut Ctx) -> String {
    let names = c.world.contact_sets.set_names();
    let choice = match &c.contacts.set_choice {
        Some(s) if ContactSets::is_internal(s) || c.world.contact_sets.is_valid(s) => s.clone(),
        _ => names.first().cloned().unwrap_or_else(|| contact_sets::ALL_SETS.to_owned()),
    };
    c.contacts.set_choice = Some(choice.clone());
    choice
}

fn list(ui: &mut egui::Ui, c: &mut Ctx, data: &super::RadarData, st: &mut PeopleUi, beside_sheet: bool) {
    let p = c.p;
    if beside_sheet {
        ui.set_max_width(ui.max_rect().width() - 10.0);
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    let set = choice(c);
    let mutable = !ContactSets::is_internal(&set);
    let world = c.world;
    set_pills(ui, c, &set);
    ui.add_space(6.0);
    // search, and "Add" (residents chosen in the picker)
    let line = Vec2::new(ui.available_width(), rows::TOOLBAR_H);
    ui.allocate_ui_with_layout(line, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let add = widgets::action_button_if(
            ui,
            p,
            "user-plus",
            aurora_i18n::tr!("common-add"),
            widgets::ActionKind::Flat,
            Vec2::new(0.0, rows::TOOLBAR_H),
            mutable,
        )
        .on_hover_text(aurora_i18n::tr!("ui-contacts-add-a-resident-to-the-selected-contact-set"));
        if add.clicked() {
            c.contacts.pick_for_set(&set);
        }
        let w = ui.available_width();
        widgets::clearable_search_box(ui, p, &mut st.filter, aurora_i18n::tr!("ui-contacts-filter-people"), w);
    });
    ui.add_space(7.0);
    let mut cache = std::mem::take(&mut st.set_cache);
    cache.update(world, &set, &st.filter);
    if cache.known < cache.total {
        ui.label(
            egui::RichText::new(aurora_i18n::tr!(
                "contacts-names-loading",
                known = aurora_i18n::integer(cache.known as i64),
                total = aurora_i18n::integer(cache.total as i64)
            ))
            .small()
            .color(p.muted),
        );
        ui.add_space(4.0);
    }
    let members = &cache.members;
    st.sel.retain(|id| cache.visible.contains(id));
    let target = st
        .anchor
        .filter(|id| st.sel.contains(id))
        .or_else(|| st.sel.last().copied())
        .or(st.sheet);
    let selected_row = target.and_then(|target| members.iter().position(|id| *id == target));
    ui.spacing_mut().item_spacing.y = RICH_GAP;
    if members.is_empty() {
        rows::empty(ui, p, aurora_i18n::tr!("ui-contacts-this-contact-set-is-empty"));
    }
    let area = widgets::search_scroll_to_row(ui, egui::ScrollArea::vertical(), selected_row, RICH_H)
        .id_salt("people-sets-rows")
        .auto_shrink([false, false])
        .show_rows(ui, RICH_H, members.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = RICH_GAP;
            for index in range {
                let id = members[index];
                row(ui, c, data, st, &set, id, &rows::display_name(world, &id), members);
            }
        });
    st.set_cache = cache;
    let drop = ui.interact(area.inner_rect, ui.scope_id().with("contact_set_drop"), Sense::hover());
    crate::ui::contacts::drag::set_target(&drop, &set);
}

/// The sets as pills with their member count (the chosen one lit), "New"
/// and the gear; the sets that do not fit are in a "+2".
fn set_pills(ui: &mut egui::Ui, c: &mut Ctx, set: &str) {
    let p = c.p;
    let world = c.world;
    let names = world.contact_sets.set_names();
    let friends: Vec<Uuid> = world.social.friends.iter().map(|f| f.id).collect();
    let count = |s: &str| {
        world.contact_sets.set(s).map_or_else(
            || world.contact_sets.members(s, &friends, |id| world.social.is_friend(id)).len(),
            |set| set.friends.len(),
        )
    };
    let new_w = widgets::filter_pill_width(ui, "plus", aurora_i18n::tr!("people-sets-new"));
    let more_w = widgets::filter_pill_width(ui, "", "+00");
    let mut chosen: Option<String> = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        // what "New" and the gear keep at the end of the line
        let mut room = ui.available_width() - new_w - 5.0 - (rows::TOOLBAR_H - 2.0) - 5.0;
        for (i, name) in names.iter().enumerate() {
            let n = aurora_i18n::integer(count(name) as i64);
            let w = widgets::dot_pill_width(ui, name, Some(&n), false);
            let rest = names.len() - i - 1;
            if w + if rest > 0 { 5.0 + more_w } else { 0.0 } > room {
                let more = widgets::filter_pill(ui, p, "", &format!("+{}", names.len() - i), false);
                menu::dropdown(&more, p, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                    for s in &names[i..] {
                        if menu::check(ui, p, "users-group", s, s == set, true) {
                            chosen = Some(s.clone());
                        }
                    }
                });
                break;
            }
            let color = world
                .contact_sets
                .set(name)
                .map_or(p.muted, |s| crate::ui::contacts::color32(s.color));
            let response = widgets::dot_pill(ui, p, color, name, Some(&n), name == set, false).0;
            crate::ui::contacts::drag::set_target(&response, name);
            if response.clicked() {
                chosen = Some(name.clone());
            }
            room -= w + 5.0;
        }
        if widgets::filter_pill(ui, p, "plus", aurora_i18n::tr!("people-sets-new"), false)
            .on_hover_text(aurora_i18n::tr!("ui-contacts-add-a-new-contact-set"))
            .clicked()
        {
            c.contacts.ask_new_set();
        }
        let gear = rows::gear(ui, p, aurora_i18n::tr!("people-sets-gear"));
        let mutable = !ContactSets::is_internal(set);
        let contacts = &mut *c.contacts;
        menu::dropdown(&gear, p, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            if menu::item_tip(
                ui,
                p,
                "file-import",
                aurora_i18n::tr!("contacts-import"),
                contacts.can_import_contact_sets(),
                aurora_i18n::tr!("contacts-import-hint"),
            ) {
                contacts.import_contact_sets(ui.ctx(), world.agent_id);
            }
            if menu::item_if(
                ui,
                p,
                "file-export",
                aurora_i18n::tr!("contacts-share-export"),
                contacts.can_export_contact_set(world, set),
            ) {
                contacts.export_contact_set(ui.ctx(), world, set);
            }
            menu::separator(ui, p);
            if menu::item_if(
                ui,
                p,
                "adjustments-filled",
                aurora_i18n::tr!("ui-contacts-configure-selected-contact-set"),
                mutable,
            ) {
                contacts.configure_set(set);
            }
            if menu::item_if(
                ui,
                p,
                "trash-filled",
                aurora_i18n::tr!("ui-contacts-delete-selected-contact-set"),
                mutable,
            ) {
                contacts.ask_remove_set(set);
            }
            menu::separator(ui, p);
            // the pseudo-sets of the contact sets' list
            for s in [
                contact_sets::ALL_SETS,
                contact_sets::NO_SETS,
                contact_sets::PSEUDONYM,
                contact_sets::EXTRA_AVS,
            ] {
                if menu::check(ui, p, "users", &crate::ui::contacts::set_label(s), s == set, true) {
                    chosen = Some(s.to_owned());
                }
            }
        });
    });
    if let Some(s) = chosen {
        c.contacts.set_choice = Some(s);
    }
}

/// The second line of a member: their nickname's real name, "not a
/// friend", or their legacy name.
fn second_line(c: &Ctx, id: &Uuid) -> String {
    let world = c.world;
    let legacy = world
        .social
        .avatar_names
        .get(id)
        .map(|n| crate::world::social::format_name(&n.legacy_first, &n.legacy_last))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| world.social.name_of(id));
    if world.contact_sets.pseudonym(id).is_some_and(|a| a != DN_REMOVED) {
        aurora_i18n::tr!("people-sets-nickname-of", name = legacy)
    } else if !world.social.is_friend(id) {
        aurora_i18n::tr!("people-sets-non-friend").to_owned()
    } else {
        legacy
    }
}

/// One member: picture and status, names, the friend's heart, or the
/// quick actions under the pointer.
#[allow(clippy::too_many_arguments)]
fn row(ui: &mut egui::Ui, c: &mut Ctx, data: &super::RadarData, st: &mut PeopleUi, set: &str, id: Uuid, name: &str, order: &[Uuid]) {
    let p = c.p;
    let world = c.world;
    let (rect, resp) = rows::allocate(ui, id, Vec2::new(ui.available_width(), RICH_H));
    widgets::reveal_search_selection(
        ui,
        &resp,
        st.anchor
            .filter(|id| st.sel.contains(id))
            .or_else(|| st.sel.last().copied())
            .or(st.sheet)
            == Some(id),
    );
    if !ui.is_rect_visible(rect) {
        rows::person_menu(&resp, p, world, &st.sel, id);
        return;
    }
    world.social.avatar_names.want_visible(&id);
    if !data.pics.contains_key(&id) {
        st.want_pic(id, ui.input(|i| i.time));
    }
    let selected = st.sel.contains(&id) || st.sheet == Some(id);
    let hot = (ui.rect_contains_pointer(rect) && !resp.dragged()) || st.row_menu == Some(id) || st.demo_hover == Some(id);
    rows::face(ui, p, rect, selected, hot);
    let left = rows::picture(ui, p, world, rect, data.pics.get(&id), id, name, false);
    let friend = world.social.is_friend(&id);
    let mutable = !ContactSets::is_internal(set);
    // under the pointer: move, take out, the menu; else the friend's heart
    let buttons: Option<Vec<(&str, &str)>> = hot.then(|| {
        let mut b = Vec::new();
        if mutable {
            b.push((
                "arrows-exchange",
                aurora_i18n::tr!("ui-contacts-move-this-resident-to-another-contact-set"),
            ));
            b.push((
                "circle-minus",
                aurora_i18n::tr!("ui-contacts-remove-this-resident-from-the-selected-contact-set"),
            ));
        }
        b.push(("dots-filled", aurora_i18n::tr!("ui-chat-more")));
        b
    });
    let widths: Vec<f32> = match &buttons {
        Some(b) => vec![rows::actions_width(b.len())],
        None if friend => vec![13.0],
        None => Vec::new(),
    };
    let end = rows::end(rect, left, &widths, 8.0, false);
    let mut tip = None;
    match (&buttons, end.items.first()) {
        (Some(buttons), Some(slot)) => hover_actions(ui, c, st, set, id, *slot, buttons),
        (None, Some(cell)) => {
            super::radar::icon(ui, "heart-filled", *cell, 13.0, p.success);
            if resp.hover_pos().is_some_and(|at| cell.expand(2.0).contains(at)) {
                tip = Some(aurora_i18n::tr!("ui-profile-aurora-friend").to_owned());
            }
        }
        _ => {}
    }
    let color = crate::ui::colors::resident_name(world, &id, p.ink);
    let second = second_line(c, &id);
    let cut = rows::person_names(
        ui,
        p,
        rect,
        left,
        end.names_right,
        name,
        color,
        &second,
        None,
        None,
        crate::world::staff::mark(world, &id),
    );
    let action = rows::select(ui, &resp, &mut st.sel, &mut st.anchor, order, id);
    if action.sheet {
        super::nearby::open_sheet(st, id);
    }
    if action.activate {
        c.actions.push(super::PeopleAction::OpenIm(id));
    }
    rows::person_menu(&resp, p, world, &st.sel, id);
    let tip = tip.or_else(|| (cut && resp.hover_pos().is_some_and(|at| at.x <= end.names_right)).then(|| name.to_owned()));
    rows::contact_tip(resp, tip.as_deref());
}

/// The residents a row's action applies to: the selection when the row is
/// part of it, else the row alone.
fn targets(st: &PeopleUi, id: Uuid) -> Vec<Uuid> {
    if st.sel.contains(&id) { st.sel.clone() } else { vec![id] }
}

fn hover_actions(ui: &mut egui::Ui, c: &mut Ctx, st: &mut PeopleUi, set: &str, id: Uuid, slot: Rect, buttons: &[(&str, &str)]) {
    let p = c.p;
    let world = c.world;
    let mut menu_open = false;
    for ((icon, _), resp) in buttons.iter().zip(rows::actions(ui, p, slot, buttons, ("people-set-act", id))) {
        match *icon {
            "arrows-exchange" => {
                let ids = targets(st, id);
                let others: Vec<String> = world.contact_sets.set_names().into_iter().filter(|s| s != set).collect();
                menu_open |= menu::dropdown(&resp, p, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                    if others.is_empty() {
                        menu::item_if(ui, p, "users-group", aurora_i18n::tr!("ui-contacts-no-other-contact-sets"), false);
                    }
                    for to in &others {
                        if menu::item(ui, p, "users-group", to) {
                            context::request(
                                ui.ctx(),
                                context::CtxAction::ContactSet(crate::ui::contacts::SetEdit::Move {
                                    from: set.to_owned(),
                                    to: to.clone(),
                                    ids: ids.clone(),
                                }),
                            );
                        }
                    }
                });
            }
            "circle-minus" if resp.clicked() => c.contacts.ask_remove_from_set(set, targets(st, id)),
            "dots-filled" => {
                let kind = if world.social.is_friend(&id) {
                    AvatarList::Friend
                } else {
                    AvatarList::Name
                };
                menu_open |= menu::dropdown(&resp, p, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                    context::avatar_list_menu(ui, p, world, id, kind);
                });
            }
            _ => {}
        }
    }
    super::keep_row_menu(st, id, menu_open);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_large_modern_list_filters_last_member_and_reveals_it_after_clearing_search() {
        let mut world = crate::world::World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        let ids: Vec<_> = (1..=5000).map(Uuid::from_u128).collect();
        world.contact_sets.add_set("Large");
        world.contact_sets.add_to_set(&ids, "Large", |_| false);
        for (n, id) in ids.iter().enumerate() {
            world.social.names.insert(*id, format!("Member {}", n + 1));
        }
        let p = crate::theme::Theme::default().palette();
        let mut contacts = crate::ui::contacts::ContactsUi::default();
        let mut friends = super::super::FriendsSettings::default();
        let mut st = PeopleUi::default();
        let mut actions = Vec::new();
        let radar = crate::world::radar::Radar::default();
        let mut settings = super::super::RadarSettings::default();
        let (voice, pics) = (std::collections::HashMap::new(), std::collections::HashMap::new());
        let data = super::super::RadarData {
            radar: &radar,
            settings: &mut settings,
            voice: &voice,
            draw_distance: 64.0,
            second_life: true,
            now: 0.0,
            pics: &pics,
        };
        let mut frames = crate::ui::widgets::elide_tests::Frames::new();
        let mut draw = |ui: &mut egui::Ui, st: &mut PeopleUi, reveal: bool| {
            ui.set_width(500.0);
            if reveal {
                let key = egui::Id::unique("search_selection_reveal").with(ui.layer_id());
                let frame = ui.ctx().cumulative_frame_nr();
                ui.ctx().data_mut(|data| data.insert_temp(key, frame));
            }
            let mut c = Ctx {
                p: &p,
                world: &world,
                contacts: &mut contacts,
                friends: &mut friends,
                images: &pics,
                actions: &mut actions,
                embedded_contacts: false,
                contact_sheet_on_click: false,
            };
            list(ui, &mut c, &data, st, false);
        };
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts = frames.run(Vec::new(), |ui| draw(ui, &mut st, false));
        }
        let drawn = texts.iter().filter(|t| t.source.starts_with("Member ")).count();
        assert!(drawn > 0 && drawn < 50, "only a viewport of members is painted");
        st.filter = "member 5000".into();
        for _ in 0..3 {
            texts = frames.run(Vec::new(), |ui| draw(ui, &mut st, false));
        }
        assert!(texts.iter().any(|t| t.source == "Member 5000"));
        st.sel = vec![ids[4999]];
        st.anchor = Some(ids[4999]);
        st.filter.clear();
        for _ in 0..3 {
            texts = frames.run(Vec::new(), |ui| draw(ui, &mut st, true));
        }
        assert!(
            texts.iter().any(|t| t.source == "Member 5000"),
            "clearing search reveals an offscreen selection"
        );
        assert_eq!(st.sel, vec![ids[4999]]);
    }

    /// Modern rows always request their picture and use only explicit
    /// UI-name attributes, resolved by set priority, rather than the old
    /// smallest-set color fallback.
    #[test]
    fn modern_set_rows_keep_pictures_and_follow_appearance_priority() {
        let mut frames = crate::ui::widgets::elide_tests::Frames::new();
        let p = crate::theme::Theme::default().palette();
        let mut world = crate::world::World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        let id = Uuid::from_u128(0xF000);
        world.social.friends.push(crate::world::social::Friend {
            id,
            online: true,
            rights_given: 1,
            rights_has: 1,
        });
        world.social.names.insert(id, "Example Person".into());
        for (name, color) in [("First", [0.7, 0.2, 0.6, 1.0]), ("Second", [0.2, 0.7, 0.6, 1.0])] {
            world.contact_sets.add_set(name);
            world.contact_sets.set_color(name, color);
            world.contact_sets.add_to_set(&[id], name, |_| true);
        }
        let mut contacts = crate::ui::contacts::ContactsUi::default();
        let mut friends = super::super::FriendsSettings::default();
        let mut st = PeopleUi::default();
        let mut actions = Vec::new();
        let radar = crate::world::radar::Radar::default();
        let mut radar_settings = super::super::RadarSettings::default();
        let (voice, pics) = (std::collections::HashMap::new(), std::collections::HashMap::new());
        let data = super::super::RadarData {
            radar: &radar,
            settings: &mut radar_settings,
            voice: &voice,
            draw_distance: 64.0,
            second_life: true,
            now: 0.0,
            pics: &pics,
        };
        for enabled in [false, true, false] {
            for name in ["First", "Second"] {
                world.contact_sets.set_visuals(
                    name,
                    crate::world::contact_sets::Visuals {
                        ui_name: enabled,
                        ..Default::default()
                    },
                );
            }
            world.contact_sets.move_priority("Second", true);
            let expected = if enabled {
                crate::interface_colors::on_panel(crate::ui::colors::set_color([0.2, 0.7, 0.6, 1.0]))
            } else {
                p.ink
            };
            let shapes = frames.shapes(Vec::new(), |ui| {
                let mut c = Ctx {
                    p: &p,
                    world: &world,
                    contacts: &mut contacts,
                    friends: &mut friends,
                    images: &pics,
                    actions: &mut actions,
                    embedded_contacts: false,
                    contact_sheet_on_click: false,
                };
                row(ui, &mut c, &data, &mut st, "First", id, "Example Person", &[id]);
            });
            let name = shapes
                .iter()
                .find_map(|shape| match shape {
                    egui::Shape::Text(t) if t.galley.text() == "Example Person" => Some(t),
                    _ => None,
                })
                .expect("the member's name is drawn");
            assert_eq!(name.galley.job.sections[0].format.color, expected);
            assert!(st.wanted_pics.contains(&id), "modern members always request their picture");
        }
    }
}

#[cfg(test)]
mod import_tests {
    use super::*;
    use crate::ui::widgets::elide_tests::Frames;

    #[test]
    fn contact_set_import_is_available_in_the_empty_people_list() {
        let world = crate::world::World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        let p = crate::theme::Theme::default().palette();
        let mut contacts = crate::ui::contacts::ContactsUi::default();
        let mut friends = super::super::FriendsSettings::default();
        let images = std::collections::HashMap::new();
        let mut actions = Vec::new();
        let mut frames = Frames::new();
        let gear = std::cell::Cell::new(egui::Pos2::ZERO);
        let mut show = |ui: &mut egui::Ui| {
            ui.set_width(400.0);
            let new_w = widgets::filter_pill_width(ui, "plus", aurora_i18n::tr!("people-sets-new"));
            gear.set(ui.cursor().min + egui::vec2(new_w + 5.0 + (rows::TOOLBAR_H - 2.0) * 0.5, (rows::TOOLBAR_H - 2.0) * 0.5));
            let mut c = Ctx {
                p: &p,
                world: &world,
                contacts: &mut contacts,
                friends: &mut friends,
                images: &images,
                actions: &mut actions,
                embedded_contacts: false,
                contact_sheet_on_click: false,
            };
            set_pills(ui, &mut c, contact_sets::ALL_SETS);
        };
        for _ in 0..3 {
            frames.run(Vec::new(), &mut show);
        }
        for pressed in [true, false] {
            frames.run(
                vec![
                    egui::Event::PointerMoved(gear.get()),
                    egui::Event::PointerButton {
                        pos: gear.get(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                &mut show,
            );
        }
        let texts = frames.run(Vec::new(), &mut show);
        assert!(
            texts.iter().any(|text| text.source == aurora_i18n::tr!("contacts-import")),
            "the migration action is visible even before any set exists"
        );
    }
}
