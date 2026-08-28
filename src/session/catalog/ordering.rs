use super::Catalog;

impl Catalog {
    pub fn reorder(&self, desired: &[String]) {
        // Normalised like every other path entering the catalog, so an order
        // given in a different spelling still names the repositories it means.
        let desired: Vec<String> = desired.iter().map(|p| Self::normalized(p)).collect();
        self.change_membership(|membership| membership.reorder(&desired));
    }
}
