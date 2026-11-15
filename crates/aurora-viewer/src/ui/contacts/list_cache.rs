//! Shared projection for both contact-set layouts. Names and sorting are
//! rebuilt on model changes, not every painted frame; typing only filters
//! already folded text. Never search raw names behind privacy filtering.
use crate::world::{World, names::NameOptions};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(PartialEq)]
struct Revision {
    account: Uuid,
    set: String,
    sets: u64,
    names: u64,
    options: NameOptions,
    friends: Vec<(Uuid, bool)>,
}

struct Entry {
    id: Uuid,
    search: String,
}

#[derive(Default)]
pub(crate) struct SetListCache {
    revision: Option<Revision>,
    refreshed: Option<Instant>,
    entries: Vec<Entry>,
    filter: Option<String>,
    pub members: Vec<Uuid>,
    pub visible: HashSet<Uuid>,
    pub total: usize,
    pub known: usize,
    #[cfg(test)]
    rebuilds: usize,
}

impl SetListCache {
    pub fn update(&mut self, world: &World, set: &str, filter: &str) {
        self.update_at(world, set, filter, Instant::now());
    }

    fn update_at(&mut self, world: &World, set: &str, filter: &str, now: Instant) {
        let social = &world.social;
        let names = &social.avatar_names;
        let revision = Revision {
            account: world.agent_id,
            set: set.to_owned(),
            sets: world.contact_sets.generation,
            names: names.generation,
            options: names.options,
            friends: social.friends.iter().map(|f| (f.id, f.online)).collect(),
        };
        // Legacy IM-header names have no generation. Refresh their fallback
        // at most once a second; display-name/privacy changes are immediate.
        let stale = self
            .refreshed
            .is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_secs(1));
        if self.revision.as_ref() != Some(&revision) || stale {
            let friends: Vec<_> = revision.friends.iter().map(|(id, _)| *id).collect();
            let friend_ids: HashSet<_> = friends.iter().copied().collect();
            let online: HashSet<_> = revision.friends.iter().filter_map(|(id, on)| on.then_some(*id)).collect();
            let members = world.contact_sets.members(set, &friends, |id| friend_ids.contains(id));
            let online_first = world.contact_sets.sorts_by_online(set);
            self.known = 0;
            let mut entries: Vec<_> = members
                .into_iter()
                .map(|id| {
                    let name = names.get(&id);
                    self.known += usize::from(name.is_some() || social.names.contains_key(&id));
                    let shown = name
                        .map(|n| n.complete(&names.options))
                        .or_else(|| social.names.get(&id).cloned())
                        .unwrap_or_else(|| "…".into());
                    let mut search = shown.to_lowercase();
                    if let Some(n) = name {
                        for text in [
                            n.display_name.clone(),
                            n.username.clone(),
                            n.user_name_for_display(&NameOptions {
                                trim_resident: false,
                                ..names.options
                            }),
                        ] {
                            search.push('\0');
                            search.push_str(&text.to_lowercase());
                        }
                    }
                    (
                        (!online_first || !online.contains(&id), crate::natural_order::key(&shown)),
                        Entry { id, search },
                    )
                })
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
            self.entries = entries.into_iter().map(|(_, entry)| entry).collect();
            self.total = self.entries.len();
            self.filter = None;
            self.revision = Some(revision);
            self.refreshed = Some(now);
            #[cfg(test)]
            {
                self.rebuilds += 1;
            }
        }
        let filter = filter.trim().to_lowercase();
        if self.filter.as_ref() != Some(&filter) {
            self.members = self
                .entries
                .iter()
                .filter(|e| filter.is_empty() || e.search.contains(&filter))
                .map(|e| e.id)
                .collect();
            self.visible = self.members.iter().copied().collect();
            self.filter = Some(filter);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::social::Friend;

    fn world() -> World {
        World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()))
    }

    #[test]
    fn contact_cache_5000_members_reuses_sort_and_filters_without_rebuilding() {
        let mut world = world();
        let ids: Vec<_> = (1..=5000).map(Uuid::from_u128).collect();
        world.contact_sets.add_set("Large");
        world.contact_sets.add_to_set(&ids, "Large", |_| false);
        for (index, id) in ids.iter().enumerate() {
            world.social.names.insert(*id, format!("Member {}", index + 1));
        }
        world.social.friends = ids[..200]
            .iter()
            .map(|id| Friend {
                id: *id,
                online: true,
                rights_given: 0,
                rights_has: 0,
            })
            .collect();
        world.contact_sets.set_sort_by_online("Large", true);
        let t = Instant::now();
        let mut cache = SetListCache::default();
        cache.update_at(&world, "Large", "", t);
        assert_eq!((cache.total, cache.known, cache.members.len()), (5000, 5000, 5000));
        assert_eq!(&cache.members[..200], &ids[..200]);
        for frame in 1..=60 {
            cache.update_at(&world, "Large", "", t + Duration::from_millis(frame * 10));
        }
        assert_eq!(cache.rebuilds, 1, "idle painting must reuse sorting and names");
        cache.update_at(&world, "Large", "  MEMBER 4999  ", t + Duration::from_millis(700));
        assert_eq!(cache.members, vec![ids[4998]]);
        assert_eq!(cache.rebuilds, 1, "typing only filters cached text");
        let mut selected = ids.clone();
        selected.retain(|id| cache.visible.contains(id));
        assert_eq!(selected, cache.members);
        cache.update_at(&world, "Large", "", t + Duration::from_millis(800));
        assert_eq!(cache.members.len(), 5000);
        // Fallback legacy names do not publish a generation.
        world.social.names.insert(ids[4998], "Renamed Member".into());
        cache.update_at(&world, "Large", "renamed", t + Duration::from_secs(1));
        assert_eq!(cache.members, vec![ids[4998]]);
    }

    #[test]
    fn contact_cache_invalidates_membership_presence_preferences_account_and_privacy() {
        let mut world = world();
        let id = Uuid::from_u128(7);
        world.contact_sets.add_set("Set");
        world.contact_sets.add_to_set(&[id], "Set", |_| false);
        world.social.avatar_names.on_legacy(id, "PrivateName", "Resident");
        let t = Instant::now();
        let mut cache = SetListCache::default();
        cache.update_at(&world, "Set", "privatename", t);
        assert_eq!(cache.members, vec![id]);
        world.social.avatar_names.set_rlva_privacy(true, HashSet::new());
        cache.update_at(&world, "Set", "privatename", t);
        assert!(cache.members.is_empty(), "a cached search cannot reveal a newly hidden name");
        world.social.avatar_names.set_rlva_privacy(false, HashSet::new());
        cache.update_at(&world, "Set", "privatename", t);
        assert_eq!(cache.members, vec![id]);
        world.social.friends.push(Friend {
            id,
            online: true,
            rights_given: 0,
            rights_has: 0,
        });
        cache.update_at(&world, "Set", "", t);
        world.social.avatar_names.options.show_usernames = false;
        cache.update_at(&world, "Set", "", t);
        world.agent_id = Uuid::from_u128(99);
        cache.update_at(&world, "Set", "", t);
        assert_eq!(cache.rebuilds, 6);
        world.contact_sets.remove_from_set(&[id], "Set", |_| true);
        cache.update_at(&world, "Set", "", t);
        assert_eq!(cache.total, 0);
    }

    #[test]
    fn contact_cache_searches_unicode_usernames_and_untrimmed_legacy_names() {
        let mut world = world();
        let id = Uuid::from_u128(8);
        world.contact_sets.add_set("Set");
        world.contact_sets.add_to_set(&[id], "Set", |_| false);
        world.social.avatar_names.on_legacy(id, "Élodie", "Resident");
        let mut cache = SetListCache::default();
        let t = Instant::now();
        for filter in ["ÉLODIE", "élodie resident", "élodie"] {
            cache.update_at(&world, "Set", filter, t);
            assert_eq!(cache.members, vec![id]);
        }
        assert_eq!(cache.rebuilds, 1);
    }
}
