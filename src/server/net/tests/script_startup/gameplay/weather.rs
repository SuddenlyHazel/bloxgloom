//! Weather scripting follows the real listener, authority and durable path.
use super::*;
use bloxgloom_host_api::gameplay::{Committed, Observer, ObserverRegistration, WeatherChanged};
use std::sync::mpsc;
struct Witness(mpsc::SyncSender<WeatherChanged>);
impl Observer for Witness {
    fn on_commit(&self, _: &Committed) {}
    fn on_weather(&self, event: &WeatherChanged) {
        let _ = self.0.try_send(*event);
    }
}
#[test]
fn luau_weather_reads_controls_and_hooks_follow_admin_commit_and_restart() {
    let fixture = Fixture::new();
    fixture.package("demo", "requires bloxgloom:actions/v1\nmodule action action.luau\nmodule observe observe.luau", "return function(h) h.register_action('demo:shift',1,'Weather','item','bloxgloom:stick','demo:action'); h.register_weather_observer('demo:weather',1,'demo:observe') end");
    std::fs::write(
        fixture.0.join("packages/demo/action.luau"),
        r#"return function(c,e)
        local w=c.weather(); local again=c.weather()
        assert(w.elapsed_ms_lo==again.elapsed_ms_lo and w.rain_mm_h==again.rain_mm_h)
        assert(not pcall(function() w.cloud=0 end))
        c.give('player',{item='bloxgloom:stick',count=1})
        if string.byte(e.arguments,1)==1 then c.admin_set_weather('storm_severe',0)
        else pcall(function() c.admin_set_weather('storm_severe',0) end) end
    end"#,
    )
    .unwrap();
    std::fs::write(fixture.0.join("packages/demo/observe.luau"), "return function(e) assert(e.kind=='WeatherChanged' and e.current.kind=='storm_severe'); assert(e.current.rain_mm_h==54 and e.set_block==nil); assert(not pcall(function() e.current.cloud=0 end)) end").unwrap();
    let mut revision = 0;
    for round in 0..2 {
        let mut state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        assert_eq!(catalog.gameplay_observers().count(), 1);
        let (tx, rx) = mpsc::sync_channel(4);
        let mut observers = Catalog::builtins();
        for r in catalog.gameplay_observers() {
            observers.register_gameplay_observer((**r).clone()).unwrap();
        }
        observers
            .register_gameplay_observer(ObserverRegistration {
                key: "witness:weather".into(),
                version: 1,
                observer: Arc::new(Witness(tx)),
            })
            .unwrap();
        state.notifications = crate::server::notifications::Lane::new(&observers).unwrap();
        if round == 0 {
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::new(crate::items::STICK, 1));
            state.inventory_store.save(PROFILE, &inventory).unwrap();
            // Native fence also denies when the script catches the exception.
            serve(state, |address| {
                let mut peer = Peer::connect(address, catalog.clone());
                peer.inventory_at(1);
                let request = peer.request(0);
                assert!(!peer.send(&request).0);
                peer.inventory_at(1);
                assert!(rx.try_recv().is_err());
            });
            state = Box::new(fixture.open().unwrap());
            state.admin_profile = Some(PROFILE);
            state.notifications = crate::server::notifications::Lane::new(&observers).unwrap();
            serve(state, |address| {
                let mut peer = Peer::connect(address, catalog);
                peer.inventory_at(1);
                let request = peer.request(1);
                assert!(peer.send(&request).0);
                peer.inventory_at(2);
                let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(
                    event.current.kind,
                    bloxgloom_host_api::gameplay::WeatherKind::StormSevere
                );
                revision = event.current.revision;
                assert!(peer.send(&request).0);
                assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
            });
        } else {
            assert_eq!(state.weather.capture().weather.revision, revision);
            assert_eq!(state.weather.capture().weather.rain_mm_h, 54.0);
            assert!(
                rx.recv_timeout(Duration::from_millis(100)).is_err(),
                "restart replayed weather hook"
            );
        }
    }
}
