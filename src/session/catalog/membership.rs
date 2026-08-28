//! Pure bookkeeping for which repository paths belong to the session.

use super::catalog_ids::{IdAssigner, Member};

#[derive(Default)]
pub(super) struct CatalogMembership {
    ids: IdAssigner,
    base: Vec<String>,
    added: Vec<String>,
    hidden: Vec<String>,
    order: Vec<String>,
}

pub(super) enum AddMembership {
    Present(String),
    TooMany,
}

impl CatalogMembership {
    pub(super) fn set_paths(&mut self, paths: Vec<String>) {
        self.base = paths;
    }

    pub(super) fn add_path(&mut self, path: String, max: usize) -> AddMembership {
        if let Some(member) = self
            .members()
            .into_iter()
            .find(|member| member.path == path)
        {
            return AddMembership::Present(member.id);
        }

        let was_hidden = self.hidden.iter().any(|hidden| hidden == &path);
        let candidate_len = self.union_paths_with_visible(&path, was_hidden).len();
        if candidate_len > max {
            return AddMembership::TooMany;
        }

        if was_hidden {
            self.hidden.retain(|hidden| hidden != &path);
            // A close forgets the old slot. A later base refresh may have put
            // the hidden path back, but an explicit reopen still belongs last.
            self.base.retain(|base| base != &path);
        }
        if !self.added.iter().any(|added| added == &path) {
            self.added.push(path.clone());
        }
        AddMembership::Present(self.ids.id_for(&path))
    }

    pub(super) fn remove_path(&mut self, path: &str) {
        for list in [&mut self.added, &mut self.base, &mut self.order] {
            list.retain(|entry| entry != path);
        }
        if !self.hidden.iter().any(|hidden| hidden == path) {
            self.hidden.push(path.to_string());
        }
    }

    pub(super) fn reorder(&mut self, desired: &[String]) {
        let served = self.union_paths();
        let mut next = Vec::with_capacity(served.len());
        for path in desired {
            if served.contains(path) && !next.contains(path) {
                next.push(path.clone());
            }
        }
        for path in served {
            if !next.contains(&path) {
                next.push(path);
            }
        }
        self.order = next;
    }

    pub(super) fn members(&mut self) -> Vec<Member> {
        self.union_paths()
            .into_iter()
            .map(|path| Member {
                id: self.ids.id_for(&path),
                path,
            })
            .collect()
    }

    fn union_paths_with_visible(&self, path: &str, was_hidden: bool) -> Vec<String> {
        let mut base = self.base.clone();
        let mut added = self.added.clone();
        let hidden: Vec<_> = self
            .hidden
            .iter()
            .filter(|hidden| !was_hidden || hidden.as_str() != path)
            .cloned()
            .collect();
        if was_hidden {
            base.retain(|base| base != path);
        }
        if !added.iter().any(|added| added == path) {
            added.push(path.to_string());
        }
        union_paths(&base, &added, &hidden, &self.order)
    }

    fn union_paths(&self) -> Vec<String> {
        union_paths(&self.base, &self.added, &self.hidden, &self.order)
    }
}

fn union_paths(
    base: &[String],
    added: &[String],
    hidden: &[String],
    order: &[String],
) -> Vec<String> {
    let mut natural = Vec::with_capacity(base.len() + added.len());
    for path in base.iter().chain(added) {
        if hidden.contains(path) || natural.contains(path) {
            continue;
        }
        natural.push(path.clone());
    }
    let mut result = Vec::with_capacity(natural.len());
    for path in order {
        if natural.contains(path) && !result.contains(path) {
            result.push(path.clone());
        }
    }
    for path in natural {
        if !result.contains(&path) {
            result.push(path);
        }
    }
    result
}

#[cfg(test)]
#[path = "membership_tests.rs"]
mod tests;
