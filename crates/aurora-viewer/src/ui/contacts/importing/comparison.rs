//! Searchable, virtualized differences inside the import preview. Membership
//! comparisons and folded name projections are cached, never saved here.
use super::*;
use crate::world::contact_sets::importing::ImportComparison;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Category {
    #[default]
    Added,
    Common,
    Retained,
}

impl Category {
    const ALL: [Self; 3] = [Self::Added, Self::Common, Self::Retained];

    fn label(self) -> &'static str {
        match self {
            Self::Added => aurora_i18n::tr!("contacts-compare-added"),
            Self::Common => aurora_i18n::tr!("contacts-compare-common"),
            Self::Retained => aurora_i18n::tr!("contacts-compare-local"),
        }
    }

    fn members(self, delta: &ImportComparison) -> &[Uuid] {
        match self {
            Self::Added => &delta.added,
            Self::Common => &delta.common,
            Self::Retained => &delta.retained,
        }
    }
}

struct Entry {
    id: Uuid,
    name: String,
    search: String,
}

struct Cache {
    revision: (u64, String, String, bool),
    delta: ImportComparison,
    category: Option<Category>,
    names: Option<(u64, NameOptions)>,
    refreshed: Option<Instant>,
    entries: Vec<Entry>,
    known: usize,
    filter: Option<String>,
    visible: Vec<usize>,
}

impl Cache {
    fn project(&mut self, world: &World, category: Category, filter: &str) {
        let names = &world.social.avatar_names;
        let revision = (names.generation, names.options);
        if self.category != Some(category)
            || self.names != Some(revision)
            || self.refreshed.is_none_or(|t| t.elapsed() >= Duration::from_secs(1))
        {
            self.known = 0;
            let mut entries: Vec<_> = category
                .members(&self.delta)
                .iter()
                .map(|id| {
                    let cached = names.get(id);
                    self.known += usize::from(cached.is_some() || world.social.names.contains_key(id));
                    let name = cached
                        .map(|n| n.complete(&names.options))
                        .or_else(|| world.social.names.get(id).cloned())
                        .unwrap_or_else(|| id.to_string());
                    let mut search = name.to_lowercase();
                    if let Some(n) = cached {
                        for text in [
                            &n.display_name,
                            &n.username,
                            &n.user_name_for_display(&NameOptions {
                                trim_resident: false,
                                ..names.options
                            }),
                        ] {
                            search.push('\0');
                            search.push_str(&text.to_lowercase());
                        }
                    }
                    (crate::natural_order::key(&name), Entry { id: *id, name, search })
                })
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
            self.entries = entries.into_iter().map(|(_, e)| e).collect();
            self.category = Some(category);
            self.names = Some(revision);
            self.refreshed = Some(Instant::now());
            self.filter = None;
        }
        let filter = filter.trim().to_lowercase();
        if self.filter.as_ref() != Some(&filter) {
            self.visible = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| (filter.is_empty() || e.search.contains(&filter)).then_some(i))
                .collect();
            self.filter = Some(filter);
        }
    }
}

#[derive(Default)]
pub(super) struct View {
    open: bool,
    category: Category,
    filter: String,
    cache: Option<Cache>,
}

impl View {
    fn refresh(&mut self, world: &World, data: &ImportData, choice: &ImportChoice) -> bool {
        let revision = (
            world.contact_sets.generation,
            choice.source.clone(),
            choice.target_name().to_owned(),
            choice.create,
        );
        if self.cache.as_ref().is_none_or(|cache| cache.revision != revision) {
            let Ok(delta) = world.contact_sets.compare_import(data, choice) else {
                self.cache = None;
                return false;
            };
            self.cache = Some(Cache {
                revision,
                delta,
                category: None,
                names: None,
                refreshed: None,
                entries: Vec::new(),
                known: 0,
                filter: None,
                visible: Vec::new(),
            });
        }
        true
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, p: &Palette, world: &World, data: &ImportData, choice: &ImportChoice) {
        let label = if self.open {
            aurora_i18n::tr!("contacts-compare-hide")
        } else {
            aurora_i18n::tr!("contacts-compare-show")
        };
        if widgets::flat_button(ui, p, label).clicked() {
            self.open = !self.open;
        }
        if !self.open || !self.refresh(world, data, choice) {
            return;
        }
        let Some(cache) = &mut self.cache else { return };
        let label = |category: Category| {
            format!(
                "{} ({})",
                category.label(),
                aurora_i18n::integer(category.members(&cache.delta).len() as i64)
            )
        };
        egui::ComboBox::from_id_salt("comparison_category")
            .selected_text(label(self.category))
            .show_ui(ui, |ui| {
                for category in Category::ALL {
                    ui.selectable_value(&mut self.category, category, label(category));
                }
            });
        if self.category == Category::Retained {
            ui.label(aurora_i18n::tr!("contacts-compare-local-help"));
        }
        let width = ui.available_width();
        widgets::clearable_search_box(ui, p, &mut self.filter, aurora_i18n::tr!("ui-contacts-filter-people"), width);
        let reset_scroll =
            cache.category != Some(self.category) || cache.filter.as_deref() != Some(self.filter.trim().to_lowercase().as_str());
        cache.project(world, self.category, &self.filter);
        if cache.known < cache.entries.len() {
            ui.label(aurora_i18n::tr!(
                "contacts-names-loading",
                known = aurora_i18n::integer(cache.known as i64),
                total = aurora_i18n::integer(cache.entries.len() as i64)
            ));
        }
        ui.label(aurora_i18n::tr!(
            "contacts-compare-results",
            shown = aurora_i18n::integer(cache.visible.len() as i64),
            total = aurora_i18n::integer(cache.entries.len() as i64)
        ));
        ui.spacing_mut().item_spacing.y = 0.0;
        let scroll = egui::ScrollArea::vertical().id_salt("comparison_members");
        let scroll = if reset_scroll {
            scroll.animated(false).vertical_scroll_offset(0.0)
        } else {
            scroll
        };
        scroll.max_height(180.0).show_rows(ui, 24.0, cache.visible.len(), |ui, range| {
            if cache.visible.is_empty() {
                ui.label(aurora_i18n::tr!("contacts-compare-empty"));
            }
            for index in range {
                let entry = &cache.entries[cache.visible[index]];
                world.social.avatar_names.want_visible(&entry.id);
                let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
                let text = egui::WidgetText::from(RichText::new(&entry.name).color(p.ink)).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Truncate),
                    rect.width(),
                    egui::TextStyle::Body,
                );
                widgets::paint_row_text(ui.painter(), text, rect.left(), rect.center().y, p.ink);
                response.on_hover_text(entry.id.to_string());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_comparison_search_cache_invalidates_for_privacy_membership_and_destination() {
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        world.contact_sets.add_set("Local");
        world.contact_sets.add_set("Other");
        let id = Uuid::from_u128(1);
        world.social.avatar_names.on_legacy(id, "Élodie", "Resident");
        let data = ImportData::from_xml(&shared_set_xml("Shared", &std::collections::BTreeSet::from([id]))).unwrap();
        let mut choice = ImportChoice {
            source: "Shared".into(),
            target: "Local".into(),
            create: false,
        };
        let mut view = View::default();
        assert!(view.refresh(&world, &data, &choice));
        let cache = view.cache.as_mut().unwrap();
        cache.project(&world, Category::Added, "ÉLODIE RESIDENT");
        assert_eq!(cache.visible.len(), 1);
        let stamp = cache.refreshed;
        cache.project(&world, Category::Added, "no match");
        assert!(cache.visible.is_empty());
        assert_eq!(cache.refreshed, stamp, "typing reuses the folded names");
        world.social.avatar_names.set_rlva_privacy(true, HashSet::new());
        cache.project(&world, Category::Added, "élodie");
        assert!(cache.visible.is_empty(), "hidden names must not survive in a cached search");
        world.contact_sets.add_to_set(&[id], "Local", |_| false);
        assert!(view.refresh(&world, &data, &choice));
        assert_eq!(view.cache.as_ref().unwrap().delta.common, vec![id]);
        choice.target = "Other".into();
        assert!(view.refresh(&world, &data, &choice));
        assert_eq!(view.cache.as_ref().unwrap().delta.added, vec![id]);
        world.contact_sets.remove_set("Other", |_| false);
        assert!(!view.refresh(&world, &data, &choice));
        assert!(view.cache.is_none());
    }
}
