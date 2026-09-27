use super::*;
impl Replicas {
    #[cfg(test)]
    pub(in crate::client) fn mobile_for_test(
        &self,
        id: crate::content::EntityTypeId,
    ) -> Option<PublicEntity> {
        self.entities
            .values()
            .flat_map(|e| e.values())
            .find(|e| e.entity_type == id)
            .cloned()
    }
    pub(in crate::client) fn aimed_mobile(
        &self,
        catalog: &Catalog,
        origin: glam::Vec3,
        direction: glam::Vec3,
        limit: f32,
    ) -> Option<&PublicEntity> {
        self.entities
            .values()
            .flat_map(|entries| entries.values())
            .filter_map(|entity| {
                let definition = catalog.mobile_entity(entity.entity_type)?;
                if definition.interaction.is_empty() {
                    return None;
                }
                let crate::protocol::PublicEntityLocation::Mobile { position } = entity.location
                else {
                    return None;
                };
                let b = definition.body;
                let min = [
                    position[0] - b.half_width,
                    position[1],
                    position[2] - b.half_width,
                ];
                let max = [
                    position[0] + b.half_width,
                    position[1] + b.height,
                    position[2] + b.half_width,
                ];
                let mut near = 0.0f32;
                let mut far = limit;
                for axis in 0..3 {
                    if direction[axis].abs() < 0.000001 {
                        if origin[axis] < min[axis] || origin[axis] > max[axis] {
                            return None;
                        }
                    } else {
                        let a = (min[axis] - origin[axis]) / direction[axis];
                        let b = (max[axis] - origin[axis]) / direction[axis];
                        near = near.max(a.min(b));
                        far = far.min(a.max(b));
                    }
                    if near > far {
                        return None;
                    }
                }
                Some((near, entity))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)))
            .map(|(_, e)| e)
    }
}
