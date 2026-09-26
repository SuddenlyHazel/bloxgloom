pub(super) use super::super::locomotion::valid_position;
use super::{BODY, EntityError, VoxelView};

pub(in crate::server) fn spawn_clear(
    view: &VoxelView,
    position: [f32; 3],
) -> Result<bool, EntityError> {
    Ok(
        valid_position(position)
            && BODY.clear(view, position)?
            && BODY.supported(view, position)?,
    )
}
