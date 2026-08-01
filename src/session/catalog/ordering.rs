use super::Catalog;

impl Catalog {
    pub(super) fn union_paths(&self) -> Vec<String> {
        let natural = {
            let base = self.base.lock().expect("catalog base poisoned");
            let added = self.added.lock().expect("catalog added poisoned");
            let hidden = self.hidden.lock().expect("catalog hidden poisoned");
            let mut natural = Vec::with_capacity(base.len() + added.len());
            for path in base.iter().chain(added.iter()) {
                if hidden.iter().any(|h| h == path) || natural.contains(path) {
                    continue;
                }
                natural.push(path.clone());
            }
            natural
        };

        let order = self.order.lock().expect("catalog order poisoned");
        if order.is_empty() {
            return natural;
        }
        let mut result = Vec::with_capacity(natural.len());
        for path in order.iter() {
            if natural.iter().any(|served| served == path) && !result.contains(path) {
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

    pub fn reorder(&self, desired: &[String]) {
        let _mutation = self.mutation.lock().expect("catalog mutation poisoned");
        let served = self.union_paths();
        let mut next = Vec::with_capacity(served.len());
        for path in desired {
            if served.iter().any(|served| served == path) && !next.contains(path) {
                next.push(path.clone());
            }
        }
        for path in &served {
            if !next.contains(path) {
                next.push(path.clone());
            }
        }
        *self.order.lock().expect("catalog order poisoned") = next;
        self.rebuild();
    }
}
