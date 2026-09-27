use super::*;

struct World {
    reads: usize,
}
impl Snapshot for World {
    fn block(&mut self, cell: Cell) -> Result<Block, Error> {
        self.reads += 1;
        if cell[0] < 0 {
            Err(Error::Unavailable(cell))
        } else {
            self.state("test:stone")
        }
    }
    fn state(&self, key: &str) -> Result<Block, Error> {
        if !matches!(key, "test:stone" | "test:air") {
            return Err(Error::UnknownContent(key.into()));
        }
        Ok(Block {
            state: key.into(),
            block_type: key.into(),
            primary_item: Some("test:stone".into()),
        })
    }
    fn item_exists(&self, key: &str) -> bool {
        key == "test:stone"
    }
}

#[test]
fn writes_capture_preimages_and_reads_see_coalesced_changes() {
    let mut world = World { reads: 0 };
    let mut ctx = Context::new(&mut world, 8);
    ctx.set_block([0; 3], "test:air").unwrap();
    assert_eq!(ctx.block([0; 3]).unwrap().state, "test:air");
    ctx.set_block([0; 3], "test:stone").unwrap();
    ctx.spawn_drop([0.5; 3], "test:stone", 1, 250).unwrap();
    let plan = ctx.finish().unwrap();
    assert_eq!(plan.blocks.len(), 1);
    assert_eq!(plan.blocks[&[0; 3]], "test:stone");
    assert_eq!(plan.drops.len(), 1);
    assert_eq!(world.reads, 1);
}

#[test]
fn ignored_failures_cannot_publish_partial_operations() {
    for failure in 0..4 {
        let mut world = World { reads: 0 };
        let mut ctx = Context::new(&mut world, if failure == 3 { 1 } else { 8 });
        ctx.set_block([0; 3], "test:air").unwrap();
        let error = match failure {
            0 => ctx.block([-1, 0, 0]).unwrap_err(),
            1 => ctx.spawn_drop([0.0; 3], "test:stone", 129, 0).unwrap_err(),
            2 => ctx.set_block([0; 3], "test:missing").unwrap_err(),
            _ => ctx.block([0; 3]).unwrap_err(),
        };
        assert_eq!(ctx.finish().unwrap_err(), error);
    }
    let mut world = World { reads: 0 };
    let mut ctx = Context::new(&mut world, 10);
    ctx.set_block([0; 3], "test:air").unwrap();
    assert_eq!(ctx.block([-1, 0, 0]), Err(Error::Unavailable([-1, 0, 0])));
    assert_eq!(ctx.finish().unwrap_err(), Error::Unavailable([-1, 0, 0]));
}
