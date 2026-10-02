use super::*;

#[test]
fn moving_events_preserve_exact_ids_revisions_and_readonly_nested_values() {
    let lua = Lua::new();
    let id = (1u64 << 60) + 3;
    let revision = (1u64 << 60) + 7;
    let pose = bloxgloom_host_api::motion::Motion {
        position: [1.0, 2.0, 3.0],
        velocity: [4.0, 5.0, 6.0],
        acceleration: [0.0; 3],
        orientation: [0.0, 0.0, 0.0, 1.0],
        revision,
        grounded: true,
    };
    let event = Event::MovingTick {
        entity: id,
        tick: revision,
        motion: pose,
    };
    let value = fields(&lua, &event).unwrap();
    assert_eq!(
        crate::server::script::handles::entity_value(value.get("entity").unwrap()).unwrap(),
        id
    );
    let motion = value.get::<Table>("motion").unwrap();
    assert_eq!(
        crate::server::script::handles::revision_value(motion.get("revision").unwrap()).unwrap(),
        revision
    );
    assert!(value.is_readonly() && motion.is_readonly());
    for name in ["position", "velocity", "acceleration", "orientation"] {
        assert!(motion.get::<Table>(name).unwrap().is_readonly());
    }
    assert!(motion.get::<bool>("grounded").unwrap());
    let impact = Event::MovingImpact {
        impact: bloxgloom_host_api::motion::Impact {
            entity: id,
            motion_revision: revision,
            tick: revision,
            position: pose.position,
            normal: [0.0, 1.0, 0.0],
            incoming_velocity: pose.velocity,
            target: bloxgloom_host_api::motion::Target::Entity {
                id: id + 1,
                revision: revision + 1,
            },
            blocked: false,
        },
    };
    let value = fields(&lua, &impact).unwrap();
    let target = value.get::<Table>("target").unwrap();
    assert!(target.is_readonly());
    assert_eq!(target.get::<String>("kind").unwrap(), "Entity");
    assert_eq!(
        crate::server::script::handles::entity_value(target.get("entity").unwrap()).unwrap(),
        id + 1
    );
    assert_eq!(
        crate::server::script::handles::revision_value(target.get("revision").unwrap()).unwrap(),
        revision + 1
    );
    for (reason, expected) in [
        (
            bloxgloom_host_api::motion::ExpiryReason::Lifetime,
            "Lifetime",
        ),
        (
            bloxgloom_host_api::motion::ExpiryReason::WorldBoundary,
            "WorldBoundary",
        ),
    ] {
        let value = fields(
            &lua,
            &Event::MovingExpiry {
                entity: id,
                tick: revision,
                motion_revision: revision,
                reason,
            },
        )
        .unwrap();
        assert_eq!(value.get::<String>("reason").unwrap(), expected);
    }
}

#[test]
fn every_public_removal_cause_preserves_its_exact_luau_context() {
    let lua = Lua::new();
    let previous = Block {
        state: "demo:lit".into(),
        block_type: "demo:block".into(),
        primary_item: Some("demo:item".into()),
        plant: false,
        supports_plant: true,
    };
    for (cause, expected) in [
        (RemovalCause::Break, "Break"),
        (RemovalCause::Transformation, "Transformation"),
        (RemovalCause::Replacement, "Replacement"),
        (RemovalCause::SupportLoss, "SupportLoss"),
        (RemovalCause::WorldEdit, "WorldEdit"),
        (RemovalCause::Burn, "Burn"),
        (RemovalCause::AnchoredBreak, "AnchoredBreak"),
    ] {
        let event = Event::BlockRemoved {
            cell: [-1, 80, 2],
            previous: previous.clone(),
            cause,
            random: (9u64 << 32) | 7,
        };
        let fields = fields(&lua, &event).unwrap();
        assert_eq!(fields.get::<String>("kind").unwrap(), "BlockRemoved");
        assert_eq!(fields.get::<String>("cause").unwrap(), expected);
        assert_eq!(fields.get::<u32>("random_lo").unwrap(), 7);
        assert_eq!(fields.get::<u32>("random_hi").unwrap(), 9);
        let cell = fields.get::<Table>("cell").unwrap();
        assert_eq!(
            [
                cell.raw_get::<i32>(1).unwrap(),
                cell.raw_get::<i32>(2).unwrap(),
                cell.raw_get::<i32>(3).unwrap()
            ],
            [-1, 80, 2]
        );
        let block = fields.get::<Table>("previous").unwrap();
        assert_eq!(block.get::<String>("state").unwrap(), "demo:lit");
        assert_eq!(block.get::<String>("block_type").unwrap(), "demo:block");
        assert_eq!(block.get::<String>("primary_item").unwrap(), "demo:item");
        assert!(block.get::<bool>("supports_plant").unwrap());
        assert!(fields.is_readonly() && cell.is_readonly() && block.is_readonly());
    }
}

#[test]
fn captured_motion_contact_preserves_exact_readonly_target_and_revision() {
    let lua = Lua::new();
    let exact = (1u64 << 60) + 9;
    let contact = bloxgloom_host_api::motion::MotionContact {
        motion_revision: exact,
        tick: exact + 1,
        target: bloxgloom_host_api::motion::Target::Entity {
            id: exact + 2,
            revision: exact + 3,
        },
        normal: [0.0, 1.0, 0.0],
    };
    let value = motion_contact(&lua, &contact).unwrap();
    assert_eq!(
        crate::server::script::handles::revision_value(value.get("motion_revision").unwrap())
            .unwrap(),
        exact
    );
    let target = value.get::<Table>("target").unwrap();
    assert_eq!(
        crate::server::script::handles::entity_value(target.get("entity").unwrap()).unwrap(),
        exact + 2
    );
    assert_eq!(
        crate::server::script::handles::revision_value(target.get("revision").unwrap()).unwrap(),
        exact + 3
    );
    assert!(
        value.is_readonly()
            && target.is_readonly()
            && value.get::<Table>("normal").unwrap().is_readonly()
    );
    lua.globals().set("contact", value).unwrap();
    assert!(lua.load("contact.target.entity = nil").exec().is_err());
}
