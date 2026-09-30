use super::*;
use bloxgloom_host_api::gameplay::{CommittedBlock, CommittedEntity, Entity};

#[test]
fn committed_observer_copies_exact_readonly_public_handles_without_writer() {
    let event = Committed {
        blocks: vec![CommittedBlock {
            cell: [-1, 80, 2],
            state: "bloxgloom:stone".into(),
        }],
        entities: vec![
            CommittedEntity::Spawned(Entity {
                id: u64::MAX,
                entity_type: "demo:actor".into(),
                position: [1., 80., 2.],
                anchor: None,
                data: vec![0, 255],
            }),
            CommittedEntity::Removed {
                id: u64::MAX - 1,
                key: "demo:actor".into(),
            },
        ],
        inventory: Some((u128::MAX, u64::MAX)),
    };
    let source = super::super::SourceModule {
        id: "demo:observe".into(),
        source: r#"return function(e)
            assert(e.kind=='Committed' and e.blocks[1].cell[1]==-1)
            assert(e.blocks[1].state=='bloxgloom:stone')
            assert(e.entities[1].kind=='Spawned' and e.entities[2].kind=='Removed')
            assert(tostring(e.entities[1].entity)=='entity:ffffffffffffffff')
            assert(e.entities[1].data==string.char(0,255))
            assert(tostring(e.inventory.profile)=='profile:ffffffffffffffffffffffffffffffff')
            assert(e.inventory.slots==nil and e.set_block==nil and e.give==nil)
            for _,v in {e,e.blocks,e.blocks[1],e.blocks[1].cell,e.entities,e.entities[1],e.inventory} do
                assert(not pcall(function() v.bad=1 end))
            end
        end"#.into(),
    };
    super::super::run_with(
        &Program::Source(source),
        Limits::default(),
        super::super::runtime::Execution::new("Committed", events::seed(&event), "advisory"),
        |lua, entry| entry.call::<()>(events::present(lua, &event)?),
    )
    .unwrap();
}
