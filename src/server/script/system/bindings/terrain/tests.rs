use super::*;
use bloxgloom_host_api::gameplay::{Block, Error};
struct World;
impl api::WorldRead for World {
    fn block(&self, cell: [i32; 3]) -> Result<Block, Error> {
        if cell[0] >= 16 {
            return Err(Error::Unavailable(cell));
        }
        Ok(Block {
            state: "test:stone".into(),
            block_type: "test:stone".into(),
            primary_item: None,
            plant: false,
            supports_plant: false,
        })
    }
}
fn call(source: &str) -> mlua::Result<api::Plan> {
    let lua = Lua::new();
    let entry = lua.load(source).eval()?;
    let context = api::Context {
        environment: None,
        tags: None,
        owner: api::Owner::Chunk([0; 3]),
        revision: 1,
        tick: 10,
        data: &[],
        world: Some(&World),
    };
    super::super::invoke(
        &lua,
        entry,
        &context,
        64,
        Capabilities {
            drops: false,
            entities: false,
            moving_entities: false,
            motion: false,
            entity_mutations: false,
        },
        &[],
        None,
    )
}
#[test]
fn box_reads_preserve_order_and_unknown_cells_poison_the_plan() {
    let plan = call(
        r#"return function(c)
        local cells=c.blocks(-1,0,0,2,1,2)
        assert(#cells==4 and cells[1].cell[1]==-1 and cells[4].cell[3]==1)
        assert(cells[1].state=='test:stone')
        assert(not pcall(function() cells[1].state='bad' end))
        return 'okay',1
    end"#,
    )
    .unwrap();
    assert_eq!(plan.data, b"okay");
    for query in [
        "c.blocks(15,0,0,2,1,1)",
        "c.blocks(0,0,0,4,4,5)",
        "c.blocks(2147483647,0,0,2,1,1)",
        "c.blocks(0,0,0,4,4,4); c.block(0,0,0)",
    ] {
        assert!(
            call(&format!(
                "return function(c) pcall(function() {query} end); return 'partial',1 end"
            ))
            .is_err(),
            "{query}"
        );
    }
}
