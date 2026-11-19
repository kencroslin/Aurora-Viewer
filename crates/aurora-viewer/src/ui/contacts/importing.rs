//! Shared membership-only export and recipient-controlled import preview.
//! File dialogs and XML parsing run off the interface thread.
use super::*;
use crate::world::contact_sets::importing::{Begun, ImportChoice, ImportData, Saving, shared_set_xml};
mod comparison;

use std::io::Read;
use std::sync::mpsc::{Receiver, TryRecvError};

pub(super) struct Job {
    account: Uuid,
    receiver: Receiver<Result<Option<ImportData>, String>>,
}

pub(super) struct ExportJob {
    account: Uuid,
    receiver: Receiver<Result<bool, String>>,
}

struct Row {
    included: bool,
    source: String,
    target: String,
    create: bool,
    comparison: comparison::View,
}

pub(super) struct Preview {
    account: Uuid,
    data: ImportData,
    rows: Vec<Row>,
    failed: bool,
}

fn available_name(name: &str, used: &mut HashSet<String>) -> String {
    let mut target = name.to_owned();
    let mut suffix = 2;
    while used.contains(&target) {
        target = format!("{name} ({suffix})");
        suffix += 1;
    }
    used.insert(target.clone());
    target
}

impl Preview {
    fn new(account: Uuid, data: ImportData, world: &World) -> Self {
        let mut used: HashSet<String> = world.contact_sets.set_names().into_iter().collect();
        let rows = data
            .set_names()
            .into_iter()
            .map(|source| Row {
                target: available_name(&source, &mut used),
                source,
                create: true,
                included: true,
                comparison: comparison::View::default(),
            })
            .collect();
        Self {
            account,
            data,
            rows,
            failed: false,
        }
    }

    fn choices(&self) -> Vec<ImportChoice> {
        self.rows
            .iter()
            .filter(|row| row.included)
            .map(|row| ImportChoice {
                source: row.source.clone(),
                target: row.target.clone(),
                create: row.create,
            })
            .collect()
    }
}

impl ContactsUi {
    pub(crate) fn can_import_contact_sets(&self) -> bool {
        self.import_job.is_none() && self.import_preview.is_none() && self.import_saving.is_none()
    }

    pub(crate) fn can_export_contact_set(&self, world: &World, set: &str) -> bool {
        self.export_job.is_none() && world.contact_sets.set(set).is_some()
    }

    pub(crate) fn export_contact_set(&mut self, ctx: &egui::Context, world: &World, name: &str) {
        if !self.can_export_contact_set(world, name) {
            return;
        }
        let Some(set) = world.contact_sets.set(name) else { return };
        // Capture only membership and its label, never the account's metadata.
        let members = set.friends.clone();
        let name = name.to_owned();
        let filename: String = name
            .chars()
            .take(60)
            .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
            .collect();
        let title = aurora_i18n::tr!("contacts-share-export").to_owned();
        let filter = aurora_i18n::tr!("contacts-import-file-type").to_owned();
        let (sender, receiver) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().name("aurora-contact-export".into()).spawn(move || {
            let result = (|| {
                let Some(path) = rfd::FileDialog::new()
                    .set_title(title)
                    .set_file_name(format!("{filename}.xml"))
                    .add_filter(filter, &["xml"])
                    .save_file()
                else {
                    return Ok(false);
                };
                let bytes = shared_set_xml(&name, &members);
                crate::atomic_file::write(&path, &bytes).map_err(|e| e.to_string())?;
                Ok(true)
            })();
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        self.export_job = Some(ExportJob {
            account: world.agent_id,
            receiver,
        });
    }

    pub(crate) fn import_contact_sets(&mut self, ctx: &egui::Context, account: Uuid) {
        if !self.can_import_contact_sets() {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let title = aurora_i18n::tr!("contacts-import").to_owned();
        let filter = aurora_i18n::tr!("contacts-import-file-type").to_owned();
        let _ = std::thread::Builder::new().name("aurora-contact-import".into()).spawn(move || {
            let result = (|| {
                let Some(path) = rfd::FileDialog::new()
                    .set_title(title)
                    .set_file_name(contact_sets::CONTACT_SETS_FILE)
                    .add_filter(filter, &["xml"])
                    .pick_file()
                else {
                    return Ok(None);
                };
                read_import(&path).map(Some)
            })();
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        self.import_job = Some(Job { account, receiver });
    }
}

fn read_import(path: &std::path::Path) -> Result<ImportData, String> {
    const MAX_BYTES: u64 = 16 * 1024 * 1024;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("cannot read contact sets: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("contact sets file exceeds 16 MiB".into());
    }
    let data = ImportData::from_xml(&bytes).map_err(str::to_owned)?;
    if data.set_names().is_empty() {
        return Err("no contact sets in this file".into());
    }
    Ok(data)
}

fn notify(world: &mut World, success: bool, text: String) {
    world.notifications.push(
        if success {
            crate::world::notifications::Kind::Info
        } else {
            crate::world::notifications::Kind::Alert
        },
        aurora_i18n::tr!("ui-contacts-contact-sets"),
        text,
        crate::world::notifications::Data::None,
    );
}

/// A confirmed preview waiting for the account's background save.
pub(super) struct PendingSave {
    preview: Preview,
    target: Option<String>,
    saving: Box<Saving>,
}

fn finish_shared_import(world: &mut World, st: &mut ContactsUi, result: Result<usize, String>) {
    match result {
        Ok(count) => {
            st.set_filter.clear();
            st.set_sel.clear();
            notify(world, true, aurora_i18n::tr!("contacts-import-success", count = count));
        }
        Err(error) => {
            log::warn!("contact import failed: {error}");
            if let Some(preview) = &mut st.import_preview {
                preview.failed = true;
            }
        }
    }
}

fn poll_save(world: &mut World, st: &mut ContactsUi) {
    let Some(PendingSave { preview, target, saving }) = st.import_saving.take() else {
        return;
    };
    if preview.account != world.agent_id {
        return;
    }
    let mut saving = Some(saving);
    let result = world.contact_sets.finish_import(&mut saving, |id| world.social.is_friend(id));
    if let Some(saving) = saving {
        st.import_saving = Some(PendingSave { preview, target, saving });
    } else if let Some(result) = result {
        if result.is_ok() {
            st.set_choice = target;
        } else {
            st.import_preview = Some(preview);
        }
        finish_shared_import(world, st, result.map_err(|e| e.to_string()));
    }
}

pub(super) fn poll(world: &mut World, st: &mut ContactsUi) {
    poll_save(world, st);
    if st.import_preview.as_ref().is_some_and(|p| p.account != world.agent_id) {
        st.import_preview = None;
    }
    if let Some(job) = &st.export_job {
        if job.account != world.agent_id {
            st.export_job = None;
        } else {
            let result = match job.receiver.try_recv() {
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("export worker stopped".into())),
                Ok(result) => Some(result),
            };
            if let Some(result) = result {
                st.export_job = None;
                match result {
                    Ok(true) => notify(world, true, aurora_i18n::tr!("contacts-share-exported").to_owned()),
                    Ok(false) => {}
                    Err(e) => {
                        log::warn!("contact export failed: {e}");
                        notify(world, false, aurora_i18n::tr!("contacts-share-export-failed").to_owned());
                    }
                }
            }
        }
    }
    let Some(job) = &st.import_job else { return };
    if job.account != world.agent_id {
        st.import_job = None;
        return;
    }
    let result = match job.receiver.try_recv() {
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("contact import worker stopped".into()),
        Ok(result) => result,
    };
    st.import_job = None;
    match result {
        Ok(Some(data)) if !data.set_names().is_empty() => st.import_preview = Some(Preview::new(world.agent_id, data, world)),
        Ok(None) => {}
        _ => notify(world, false, aurora_i18n::tr!("contacts-import-failed").to_owned()),
    }
}

pub(super) fn show_preview(ctx: &egui::Context, p: &Palette, world: &mut World, st: &mut ContactsUi) {
    let Some(mut preview) = st.import_preview.take() else { return };
    if preview.account != world.agent_id {
        return;
    }
    let names = world.contact_sets.set_names();
    let mut done = None;
    let modal = widgets::Modal::new(egui::Id::unique("contact_share_preview")).show(ctx, |ui| {
        ui.set_width(540.0_f32.min(ctx.content_rect().width() - 48.0).max(240.0));
        ui.label(
            RichText::new(aurora_i18n::tr!("contacts-share-preview"))
                .strong()
                .size(15.0)
                .color(p.ink),
        );
        ui.label(aurora_i18n::tr!("contacts-share-preserve"));
        egui::ScrollArea::vertical()
            .max_height((ctx.content_rect().height() - 240.0).clamp(120.0, 560.0))
            .show(ui, |ui| {
                for (index, row) in preview.rows.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        widgets::checkbox(ui, p, &mut row.included, &row.source);
                        ui.add_enabled_ui(row.included, |ui| {
                            egui::ComboBox::from_id_salt("mode")
                                .selected_text(if row.create {
                                    aurora_i18n::tr!("contacts-share-new")
                                } else {
                                    aurora_i18n::tr!("contacts-share-merge")
                                })
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(row.create, aurora_i18n::tr!("contacts-share-new")).clicked() && !row.create {
                                        row.create = true;
                                        row.target = available_name(&row.source, &mut names.iter().cloned().collect());
                                    }
                                    if ui
                                        .add_enabled_ui(!names.is_empty(), |ui| {
                                            ui.selectable_label(!row.create, aurora_i18n::tr!("contacts-share-merge"))
                                        })
                                        .inner
                                        .clicked()
                                    {
                                        if row.create {
                                            row.target = names
                                                .iter()
                                                .find(|name| **name == row.source)
                                                .or_else(|| names.first())
                                                .cloned()
                                                .unwrap_or_default();
                                        }
                                        row.create = false;
                                    }
                                });
                            if row.create {
                                ui.add(
                                    egui::TextEdit::singleline(&mut row.target)
                                        .desired_width(f32::INFINITY)
                                        .hint_text(aurora_i18n::tr!("contacts-share-name")),
                                );
                            } else {
                                egui::ComboBox::from_id_salt("destination")
                                    .selected_text(&row.target)
                                    .show_ui(ui, |ui| {
                                        for name in &names {
                                            ui.selectable_value(&mut row.target, name.clone(), name);
                                        }
                                    });
                            }
                            let choice = ImportChoice {
                                source: row.source.clone(),
                                target: row.target.clone(),
                                create: row.create,
                            };
                            let count = world
                                .contact_sets
                                .preview_import(&preview.data, std::slice::from_ref(&choice))
                                .ok()
                                .and_then(|v| v.first().copied());
                            ui.label(aurora_i18n::tr!(
                                "contacts-share-counts",
                                members = aurora_i18n::integer(preview.data.member_count(&row.source) as i64),
                                added = aurora_i18n::integer(count.map_or(0, |c| c.added) as i64),
                                duplicates = aurora_i18n::integer(count.map_or(0, |c| c.duplicates) as i64)
                            ));
                            if let Some(count) = count {
                                if !row.create {
                                    ui.label(aurora_i18n::tr!(
                                        "contacts-compare-retained",
                                        count = aurora_i18n::integer(count.retained as i64)
                                    ));
                                }
                                row.comparison.show(ui, p, world, &preview.data, &choice);
                            }
                        });
                        widgets::divider(ui, p);
                    });
                }
            });
        let valid = world.contact_sets.preview_import(&preview.data, &preview.choices()).is_ok();
        if !valid {
            ui.label(aurora_i18n::tr!("contacts-share-invalid"));
        }
        if preview.failed {
            ui.label(aurora_i18n::tr!("contacts-import-failed"));
        }
        ui.horizontal(|ui| {
            if widgets::action_button_if(
                ui,
                p,
                "file-import",
                aurora_i18n::tr!("contacts-share-apply"),
                widgets::ActionKind::Primary,
                Vec2::new(0.0, 28.0),
                valid,
            )
            .clicked()
            {
                done = Some(true);
            }
            if widgets::flat_button(ui, p, aurora_i18n::tr!("common-cancel")).clicked() {
                done = Some(false);
            }
        });
    });
    if modal.should_close() && done.is_none() {
        done = Some(false);
    }
    match done {
        Some(false) => {}
        Some(true) => {
            let choices = preview.choices();
            match world
                .contact_sets
                .begin_import_choices(&preview.data, &choices, |id| world.social.is_friend(id))
            {
                Ok(Begun::Done(count)) => {
                    st.set_choice = choices.first().map(|c| c.target_name().to_owned());
                    finish_shared_import(world, st, Ok(count));
                }
                Ok(Begun::Saving(saving)) => {
                    let target = choices.first().map(|c| c.target_name().to_owned());
                    st.import_saving = Some(PendingSave { preview, target, saving });
                }
                Err(e) => {
                    log::warn!("contact import failed: {e}");
                    preview.failed = true;
                    st.import_preview = Some(preview);
                }
            }
        }
        None => st.import_preview = Some(preview),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()))
    }

    fn parsed() -> ImportData {
        ImportData::from_xml(
            br#"<llsd><map><key>groupname</key><string>Imported</string><key>notices</key><boolean>1</boolean></map></llsd>"#,
        )
        .expect("individual export")
    }

    #[test]
    fn contact_set_import_completion_updates_shared_state_once() {
        let mut world = world();
        let mut st = ContactsUi {
            set_filter: "old filter".into(),
            ..Default::default()
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        st.import_job = Some(Job {
            account: world.agent_id,
            receiver,
        });
        assert!(sender.send(Ok(Some(parsed()))).is_ok());
        poll(&mut world, &mut st);
        assert!(!world.contact_sets.is_valid("Imported"));
        assert!(st.import_preview.is_some());
        assert_eq!(st.set_filter, "old filter");
        assert!(!st.can_import_contact_sets());
        let generation = world.contact_sets.generation;
        poll(&mut world, &mut st);
        assert_eq!(world.contact_sets.generation, generation);
    }

    #[test]
    fn contact_set_import_cancel_failure_and_account_change_preserve_contacts() {
        let mut world = world();
        world.contact_sets.add_set("Existing");
        for result in [Ok(None), Err("invalid file".into()), Ok(Some(parsed()))] {
            let account = if result.as_ref().is_ok_and(|v| v.is_some()) {
                Uuid::from_u128(42)
            } else {
                world.agent_id
            };
            let mut st = ContactsUi::default();
            let (sender, receiver) = std::sync::mpsc::channel();
            st.import_job = Some(Job { account, receiver });
            assert!(sender.send(result).is_ok());
            let generation = world.contact_sets.generation;
            poll(&mut world, &mut st);
            assert_eq!(world.contact_sets.set_names(), ["Existing"]);
            assert_eq!(world.contact_sets.generation, generation);
            assert!(st.can_import_contact_sets());
        }
    }
    fn frame(ctx: &egui::Context, world: &mut World, st: &mut ContactsUi, events: Vec<egui::Event>) -> Vec<egui::epaint::ClippedShape> {
        let p = crate::theme::Theme::default().palette();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| show_preview(ui.ctx(), &p, world, st),
        );
        out.textures_delta.clear();
        out.shapes
    }

    fn text_position(shapes: &[egui::epaint::ClippedShape], text: &str) -> egui::Pos2 {
        fn find(shape: &egui::Shape, text: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos + t.galley.size() / 2.0),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
                _ => None,
            }
        }
        shapes
            .iter()
            .find_map(|s| find(&s.shape, text))
            .unwrap_or_else(|| panic!("missing preview text: {text}"))
    }

    fn click(ctx: &egui::Context, world: &mut World, st: &mut ContactsUi, at: egui::Pos2) {
        frame(
            ctx,
            world,
            st,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            ctx,
            world,
            st,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }

    fn preview_fixture() -> (egui::Context, World, ContactsUi) {
        let ctx = egui::Context::default();
        crate::theme::Theme::default().apply(&ctx, 1.0);
        let mut world = world();
        world.contact_sets.add_set("Work");
        world
            .contact_sets
            .add_to_set(&[Uuid::from_u128(1), Uuid::from_u128(3)], "Work", |_| false);
        world.contact_sets.set_color("Work", [1.0, 0.0, 0.0, 1.0]);
        world.contact_sets.set_pseudonym(&[Uuid::from_u128(1)], "Private", |_| false);
        let members = std::collections::BTreeSet::from([Uuid::from_u128(1), Uuid::from_u128(2)]);
        let data = ImportData::from_xml(&shared_set_xml("Work", &members)).expect("share");
        let st = ContactsUi {
            import_preview: Some(Preview::new(world.agent_id, data, &world)),
            ..Default::default()
        };
        (ctx, world, st)
    }

    #[test]
    fn contact_sharing_rendered_preview_creates_new_or_merges_only_after_confirmation() {
        for merge in [false, true] {
            let (ctx, mut world, mut st) = preview_fixture();
            let before = world.contact_sets.set("Work").expect("local").clone();
            frame(&ctx, &mut world, &mut st, vec![]);
            let shapes = frame(&ctx, &mut world, &mut st, vec![]);
            assert_eq!(st.import_preview.as_ref().expect("preview").rows[0].target, "Work (2)");
            assert_eq!(world.contact_sets.set_names(), vec!["Work"]);
            if merge {
                let mode = text_position(&shapes, aurora_i18n::tr!("contacts-share-new"));
                click(&ctx, &mut world, &mut st, mode);
                let shapes = frame(&ctx, &mut world, &mut st, vec![]);
                let option = text_position(&shapes, aurora_i18n::tr!("contacts-share-merge"));
                click(&ctx, &mut world, &mut st, option);
                assert!(!st.import_preview.as_ref().expect("preview").rows[0].create);
            }
            let shapes = frame(&ctx, &mut world, &mut st, vec![]);
            let summary = aurora_i18n::tr!(
                "contacts-share-counts",
                members = "2",
                added = if merge { "1" } else { "2" },
                duplicates = if merge { "1" } else { "0" }
            );
            text_position(&shapes, &summary);
            assert_eq!(world.contact_sets.set("Work"), Some(&before), "preview has no effect");
            let apply = text_position(&shapes, aurora_i18n::tr!("contacts-share-apply"));
            click(&ctx, &mut world, &mut st, apply);
            assert!(st.import_preview.is_none());
            assert_eq!(world.contact_sets.pseudonym(&Uuid::from_u128(1)), Some("Private"));
            if merge {
                assert_eq!(world.contact_sets.set("Work").expect("merged").friends.len(), 3);
                assert_eq!(world.contact_sets.set("Work").expect("merged").color, before.color);
                assert!(!world.contact_sets.is_valid("Work (2)"));
            } else {
                assert_eq!(world.contact_sets.set("Work"), Some(&before));
                assert_eq!(world.contact_sets.set("Work (2)").expect("new").friends.len(), 2);
            }
        }
    }

    #[test]
    fn contact_sharing_preview_cancel_and_account_change_do_not_import() {
        let (ctx, mut world, mut st) = preview_fixture();
        let generation = world.contact_sets.generation;
        frame(&ctx, &mut world, &mut st, vec![]);
        let shapes = frame(&ctx, &mut world, &mut st, vec![]);
        let cancel = text_position(&shapes, aurora_i18n::tr!("common-cancel"));
        click(&ctx, &mut world, &mut st, cancel);
        assert!(st.import_preview.is_none());
        assert!(st.can_import_contact_sets());
        assert_eq!(world.contact_sets.generation, generation);
        let data = parsed();
        st.import_preview = Some(Preview::new(Uuid::from_u128(42), data, &world));
        poll(&mut world, &mut st);
        assert!(st.import_preview.is_none());
        assert_eq!(world.contact_sets.generation, generation);
    }

    #[test]
    fn contact_sharing_file_reader_bounds_size_and_round_trips_membership() {
        let path = std::env::temp_dir().join(format!("aurora-shared-{}.xml", Uuid::new_v4()));
        let ids = std::collections::BTreeSet::from([Uuid::from_u128(1), Uuid::from_u128(2)]);
        crate::atomic_file::write(&path, &shared_set_xml("Shared", &ids)).expect("export");
        assert_eq!(read_import(&path).expect("import").member_count("Shared"), 2);
        std::fs::write(&path, b"broken XML").expect("write");
        assert!(read_import(&path).is_err());
        let f = std::fs::File::create(&path).expect("file");
        f.set_len(16 * 1024 * 1024 + 1).expect("size");
        assert!(read_import(&path).is_err());
        std::fs::remove_file(path).expect("cleanup");
    }
    #[test]
    fn contact_comparison_rendered_preview_shows_three_groups_and_keeps_local_members() {
        let (ctx, mut world, mut st) = preview_fixture();
        for (id, name) in [(1, "CommonMember"), (2, "NewMember"), (3, "LocalMember")] {
            world.social.avatar_names.on_legacy(Uuid::from_u128(id), name, "Resident");
        }
        let generation = world.contact_sets.generation;
        frame(&ctx, &mut world, &mut st, vec![]);
        let shapes = frame(&ctx, &mut world, &mut st, vec![]);
        click(
            &ctx,
            &mut world,
            &mut st,
            text_position(&shapes, aurora_i18n::tr!("contacts-share-new")),
        );
        let shapes = frame(&ctx, &mut world, &mut st, vec![]);
        click(
            &ctx,
            &mut world,
            &mut st,
            text_position(&shapes, aurora_i18n::tr!("contacts-share-merge")),
        );
        let shapes = frame(&ctx, &mut world, &mut st, vec![]);
        text_position(&shapes, &aurora_i18n::tr!("contacts-compare-retained", count = "1"));
        click(
            &ctx,
            &mut world,
            &mut st,
            text_position(&shapes, aurora_i18n::tr!("contacts-compare-show")),
        );
        let shapes = frame(&ctx, &mut world, &mut st, vec![]);
        text_position(&shapes, "NewMember");
        for (key, member) in [
            ("contacts-compare-common", "CommonMember"),
            ("contacts-compare-local", "LocalMember"),
        ] {
            let previous = if key == "contacts-compare-common" {
                "contacts-compare-added"
            } else {
                "contacts-compare-common"
            };
            let shapes = frame(&ctx, &mut world, &mut st, vec![]);
            let pos = text_position(&shapes, &format!("{} (1)", aurora_i18n::text(previous)));
            click(&ctx, &mut world, &mut st, pos);
            let shapes = frame(&ctx, &mut world, &mut st, vec![]);
            click(
                &ctx,
                &mut world,
                &mut st,
                text_position(&shapes, &format!("{} (1)", aurora_i18n::text(key))),
            );
            let shapes = frame(&ctx, &mut world, &mut st, vec![]);
            text_position(&shapes, member);
        }
        assert_eq!(world.contact_sets.generation, generation, "browsing comparison is read-only");
        let shapes = frame(&ctx, &mut world, &mut st, vec![]);
        text_position(&shapes, aurora_i18n::tr!("contacts-compare-local-help"));
        click(
            &ctx,
            &mut world,
            &mut st,
            text_position(&shapes, aurora_i18n::tr!("contacts-share-apply")),
        );
        assert!(st.import_preview.is_none());
        assert_eq!(
            world.contact_sets.set("Work").unwrap().friends,
            (1..=3).map(Uuid::from_u128).collect()
        );
    }
}
