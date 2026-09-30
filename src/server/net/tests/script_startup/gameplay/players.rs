//! Exact online identities through real nonblocking admission and action dispatch.
use super::*;

#[test]
fn luau_player_directory_uses_claimed_profiles_and_exact_sessions_over_listener() {
    let fixture = Fixture::new();
    fixture.action(
        "return function(h) h.register_action('demo:shift',1,'Who','empty',nil,'demo:action') end",
        r#"
        return function(c,e)
            local list=c.players()
            assert(#list==1)
            local me=c.player_by_profile(c.player_profile)
            assert(me and me.profile==list[1].profile and me.name=='luau-action')
            assert(me.identity_trust=='claimed_profile' and me.online)
            assert(me.entity~=nil and c.player_by_session(me.session).profile==me.profile)
            assert(tostring(me.session):match('^session:'))
            assert(not pcall(function() me.position[1]=99 end))
            assert(not pcall(function() list[2]=me end))
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            if string.byte(e.arguments,1)==1 then
                pcall(function() c.player_by_session(1) end)
            end
        end
    "#,
    );
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let initial_revision = peer.inventory.revision;
        let query = peer.request(0);
        let (accepted, reason) = peer.send(&query);
        assert!(accepted, "{reason}");
        let deadline = Instant::now() + Duration::from_secs(10);
        while peer.inventory.revision == initial_revision {
            peer.read(deadline);
        }
        let before = peer.inventory.clone();
        let forged = peer.request(1);
        let (accepted, reason) = peer.send(&forged);
        assert!(
            !accepted && reason.contains("expected session ID"),
            "{reason}"
        );
        assert_eq!(
            peer.inventory, before,
            "caught forged identity published a grant"
        );
    });
}
