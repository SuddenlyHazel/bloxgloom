use super::*;

fn version_two(source: &str) -> Session {
    let sample = sample();
    let assets = change_json(&sample.packages()["uidemo"].ui_assets, "welcome", |doc| {
        doc["version"] = 2.into()
    });
    let bytes = encode_source(&assets, source.as_bytes(), 1);
    let bundle =
        ClientBundle::decode_verify(&bytes, CacheKey::from_bytes(Sha256::digest(&bytes).into()))
            .unwrap();
    let mut session = Session::new(Arc::clone(bundle.ui().unwrap()));
    session.resize(640, 360, 1.0);
    session
}
fn index(session: &Session, id: &str) -> usize {
    session
        .document()
        .nodes
        .iter()
        .position(|n| n.id == format!("uidemo:welcome/{id}"))
        .unwrap()
}

#[test]
fn dynamic_children_reorder_preserves_editable_values_and_exact_focus_then_removal_clears_it() {
    let mut session = version_two(
        r#"
        return function(e)
            if e.state=='' then
                return {{op='state',value='created'},{op='children',node='uidemo:welcome/root',nodes={
                    {id='check',kind='checkbox',style='uidemo:field',text='Ready',checked=true},
                    {id='name',kind='input',style='uidemo:field',text='replacement default',event='uidemo:changed'}
                }}}
            elseif e.state=='created' then
                return {{op='state',value='reordered'},{op='children',node='uidemo:welcome/root',nodes={
                    {id='name',kind='input',style='uidemo:field',text='another default',event='uidemo:changed'},
                    {id='check',kind='checkbox',style='uidemo:field',text='Ready again',checked=false}
                }}}
            else
                return {{op='children',node='uidemo:welcome/root',nodes={
                    {id='check',kind='checkbox',style='uidemo:field',text='Remaining',checked=false}
                }}}
            end
        end
    "#,
    );
    session.apply_egui(EguiIntent::Focus(4));
    assert!(session.dispatch_event("uidemo:create".into(), String::new()));
    session.wait_for_presentation().unwrap();
    assert_eq!(session.document().nodes.len(), 3);
    assert_eq!(session.inputs[index(&session, "name")], "Moss & stone");
    assert_eq!(session.focused_id(), Some("uidemo:welcome/name"));
    let name = index(&session, "name");
    session.apply_egui(EguiIntent::Input(name, "edited café".into()));
    session.wait_for_presentation().unwrap();
    assert_eq!(index(&session, "name"), 1);
    assert_eq!(session.inputs[1], "edited café");
    assert_eq!(session.inputs[index(&session, "check")], "true");
    assert_eq!(session.focused_id(), Some("uidemo:welcome/name"));
    assert!(session.dispatch_event("uidemo:remove".into(), String::new()));
    session.wait_for_presentation().unwrap();
    assert_eq!(session.document().nodes.len(), 2);
    assert!(session.focused_id().is_none());
    assert_eq!(session.inputs[index(&session, "check")], "true");
}

#[test]
fn dynamic_mixed_replies_reject_foreign_resources_colliding_ids_and_bad_values_atomically() {
    for commands in [
        "{op='children',node='uidemo:welcome/root',nodes={{id='name',kind='input',style='other:field'}}}",
        "{op='children',node='uidemo:welcome/root',nodes={{id='root',kind='label',style='uidemo:label',text='collision'}}}",
        "{op='children',node='uidemo:welcome/root',nodes={{id='check',kind='checkbox',style='uidemo:field',checked=false}}},{op='value',node='uidemo:welcome/check',value='yes'}",
        "{op='children',node='uidemo:welcome/root',nodes={{id='choice',kind='select',style='uidemo:field',options={{key='one',label='One'}},selected='missing'}}}",
        "{op='children',node='other:welcome/root',nodes={}}",
    ] {
        let source = format!(
            "return function(e) return {{{{op='state',value='partial'}},{{op='text',node='uidemo:welcome/title',value='partial'}},{commands}}} end"
        );
        let mut session = version_two(&source);
        session.apply_egui(EguiIntent::Focus(4));
        let ids = session
            .document()
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>();
        assert!(session.dispatch_event("uidemo:invalid".into(), String::new()));
        assert!(
            session.wait_for_presentation().is_err(),
            "accepted {commands}"
        );
        assert_eq!(session.state, "");
        assert_eq!(session.text_at(1), "Welcome to the garden");
        assert_eq!(
            session
                .document()
                .nodes
                .iter()
                .map(|n| n.id.clone())
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(session.inputs[4], "Moss & stone");
        assert_eq!(session.focused_id(), Some("uidemo:welcome/name"));
    }
}

#[test]
fn guarded_egui_input_and_activation_cannot_target_replacement_or_reset_widgets() {
    let mut session = version_two(
        r#"
        return function(e)
            if e.event=='uidemo:replace' then
                return {{op='children',node='uidemo:welcome/root',nodes={
                    {id='a',kind='label',style='uidemo:label'},
                    {id='b',kind='label',style='uidemo:label'},
                    {id='c',kind='label',style='uidemo:label'},
                    {id='replacement',kind='input',style='uidemo:field',text='fresh',event='uidemo:changed'},
                    {id='button',kind='button',style='uidemo:button',text='New',event='uidemo:clicked'}
                }}}
            end
            return {{op='state',value='unexpected activation'}}
        end
    "#,
    );
    let old = session.tree_generation;
    assert!(session.dispatch_event("uidemo:replace".into(), String::new()));
    session.wait_for_presentation().unwrap();
    assert_ne!(session.tree_generation, old);
    let sequence = session.sequence;
    for intent in [
        EguiIntent::Input(4, "stale".into()),
        EguiIntent::Activate(5),
    ] {
        session.apply_egui(EguiIntent::Guarded {
            generation: old,
            intent: Box::new(intent),
        });
    }
    assert_eq!(session.inputs[4], "fresh");
    assert_eq!(session.sequence, sequence);
    assert!(session.pending.is_none());
    assert_eq!(session.state, "");
    let before_switch = session.tree_generation;
    session.next_document();
    for intent in [
        EguiIntent::Input(4, "stale reset".into()),
        EguiIntent::Activate(5),
    ] {
        session.apply_egui(EguiIntent::Guarded {
            generation: before_switch,
            intent: Box::new(intent),
        });
    }
    assert_eq!(session.inputs[4], "Moss & stone");
    assert_eq!(session.sequence, sequence);
    assert!(session.pending.is_none());
}

#[test]
fn incoming_player_text_updates_follow_current_widget_ids_after_dynamic_replacement() {
    let mut session = version_two(
        r#"return function(e) return {{op='children',node='uidemo:welcome/root',nodes={
        {id='name',kind='input',style='uidemo:field',text='new default'},
        {id='local',kind='label',style='uidemo:label',text='Keep local'}
    }}} end"#,
    );
    assert!(session.dispatch_event("uidemo:replace".into(), String::new()));
    session.wait_for_presentation().unwrap();
    let mut update = crate::client::startup::State::default();
    update
        .texts
        .insert("uidemo:welcome/name".into(), "server name".into());
    update
        .texts
        .insert("uidemo:welcome/title".into(), "removed title".into());
    session.apply_player_update(&update).unwrap();
    assert_eq!(session.inputs[index(&session, "name")], "server name");
    assert_eq!(session.text_at(index(&session, "local")), "Keep local");
    assert_eq!(session.document().nodes.len(), 3);
}

#[test]
fn worker_receives_readonly_typed_checkbox_slider_selection_and_multiline_values() {
    let mut session = version_two(
        r#"
        return function(e)
            if e.event=='uidemo:create' then
                return {{op='children',node='uidemo:welcome/root',nodes={
                    {id='check',kind='checkbox',style='uidemo:field',checked=true},
                    {id='slide',kind='slider',style='uidemo:field',min=0,max=1,value=0.5,step=0.25},
                    {id='select',kind='select',style='uidemo:field',options={{key='one',label='One'}},selected='one'},
                    {id='notes',kind='multiline_input',style='uidemo:field',text='line one\nline two'}
                }}}
            end
            assert(e.values['uidemo:welcome/check']==true)
            assert(type(e.values['uidemo:welcome/slide'])=='number' and e.values['uidemo:welcome/slide']==0.5)
            assert(e.values['uidemo:welcome/select']=='one')
            assert(e.values['uidemo:welcome/notes']=='line one\nline two')
            assert(not pcall(function()e.values['uidemo:welcome/check']=false end))
            assert(not pcall(function()e.values={} end))
            return {{op='state',value='typed and readonly'}}
        end
    "#,
    );
    assert!(session.dispatch_event("uidemo:create".into(), String::new()));
    session.wait_for_presentation().unwrap();
    assert!(session.dispatch_event("uidemo:inspect".into(), String::new()));
    session.wait_for_presentation().unwrap();
    assert_eq!(session.state, "typed and readonly");
    assert_eq!(session.inputs[index(&session, "check")], "true");
}

#[test]
fn oversized_dynamic_tree_depth_and_retained_text_fail_without_partial_state() {
    for build in [
        "for i=1,256 do nodes[i]={id='n'..i,kind='label',style='uidemo:label'} end",
        "for i=1,16 do nodes[i]={id='n'..i,kind='panel',style='uidemo:panel',parent=i>1 and i-2 or nil} end",
        "for i=1,20 do nodes[i]={id='n'..i,kind='multiline_input',style='uidemo:field',text=string.rep('x',1000)} end",
        "for i=1,33 do nodes[i]={id='n'..i,kind='multiline_input',style='uidemo:field',text=''} end",
    ] {
        let source = format!(
            "return function(e) local nodes={{}}; {build}; return {{{{op='state',value='partial'}},{{op='children',node='uidemo:welcome/root',nodes=nodes}}}} end"
        );
        let mut session = version_two(&source);
        assert!(session.dispatch_event("uidemo:oversized".into(), String::new()));
        assert!(session.wait_for_presentation().is_err(), "accepted {build}");
        assert_eq!(session.state, "");
        assert_eq!(session.document().nodes.len(), 6);
        assert_eq!(session.inputs[4], "Moss & stone");
    }
}

#[test]
fn replica_queue_redacts_another_packages_active_document_values_state_and_texts() {
    for owner in ["uidemo", "other"] {
        let mut session = version_two("return function(e)return {} end");
        session.state = "private session state".into();
        session.inputs[4] = "private edited input".into();
        session.texts[1] = "private title".into();
        let assertions = if owner == "uidemo" {
            "assert(e.state=='private session state'); assert(e.texts['uidemo:welcome/title']=='private title'); assert(e.values['uidemo:welcome/name']=='private edited input')"
        } else {
            "assert(e.state==''); assert(next(e.texts)==nil); assert(next(e.values)==nil); assert(e.values['uidemo:welcome/name']==nil)"
        };
        session.startup.replica = Some(Arc::new(crate::client::presentation::Script {
            module: format!("{owner}@1.0.0:replica"),
            source: format!(
                "return function(e) assert(e.event=='replica:entities'); {assertions}; assert(not pcall(function() e.values['uidemo:welcome/name']='injected' end)); return {{}} end"
            ),
        }));
        let sequence = session.sequence;
        session.replica_event("replica:entities", "total=0".into());
        assert_eq!(session.sequence, sequence + 1);
        session.wait_for_presentation().unwrap();
        assert_eq!(session.state, "private session state");
        assert_eq!(session.inputs[4], "private edited input");
        assert_eq!(session.texts[1], "private title");
        assert!(session.replica_events.is_empty());
    }
}

#[test]
fn player_text_update_cannot_overflow_a_near_capacity_dynamic_document() {
    let mut session = version_two(
        r#"
        return function(e)
            local nodes={}
            for i=1,31 do nodes[i]={id='notes'..i,kind='multiline_input',style='uidemo:field'} end
            local options={}
            for i=1,7 do options[i]={key='o'..i,label=string.rep('x',120)} end
            nodes[32]={id='choice',kind='select',style='uidemo:field',options=options}
            nodes[33]={id='title',kind='label',style='uidemo:label'}
            return {{op='children',node='uidemo:welcome/root',nodes=nodes}}
        end
    "#,
    );
    assert!(session.dispatch_event("uidemo:create".into(), String::new()));
    session.wait_for_presentation().unwrap();
    assert_eq!(session.text_at(index(&session, "title")), "");
    let mut update = crate::client::startup::State::default();
    update
        .texts
        .insert("uidemo:welcome/title".into(), "x".repeat(128));
    assert!(session.apply_player_update(&update).is_err());
    assert_eq!(session.text_at(index(&session, "title")), "");
    assert!(!session.startup.texts.contains_key("uidemo:welcome/title"));
}
