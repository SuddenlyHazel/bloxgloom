use super::*;
#[path = "benchmarks.rs"]
mod benchmarks;
#[test]
fn ordinary_libraries_and_seeded_random_work_in_fresh_sandboxes() {
    let execute = |seed| {
        let (lua, diagnostics) =
            create("demo@1.0.0:main", Execution::new("test", seed, "test")).unwrap();
        let result: Vec<f64> = lua.load(r#"
            assert(math.floor(2.9)==2 and math.sqrt(9)==3 and math.clamp(4,0,2)==2)
            assert(utf8.len('你好')==2 and bit32.band(7,3)==3)
            local b=buffer.create(8); buffer.writeu32(b,0,123456); assert(buffer.readu32(b,0)==123456)
            assert(vector.dot(vector.create(1,2,3),vector.create(1,0,0))==1)
            local exact=integer.fromstring('9007199254740993')
            assert(tostring(integer.add(exact,integer.create(1)))=='9007199254740994')
            local co=coroutine.create(function() coroutine.yield(12); return 34 end)
            local ok,a=coroutine.resume(co); assert(ok and a==12)
            local ok,z=coroutine.resume(co); assert(ok and z==34)
            assert(type(debug.info)=='function' and type(debug.traceback)=='function')
            assert(os.difftime(12,3)==9 and os.time==nil and os.clock==nil and os.date==nil)
            return {math.random(), math.random(1,1000000), math.random(-1000000,1000000)}
        "#).eval().unwrap();
        diagnostics.finish("evaluated");
        result
    };
    assert_eq!(execute(123), execute(123));
    assert_ne!(execute(123), execute(124));
}
#[test]
fn author_reseeding_keeps_standard_math_random_semantics() {
    let (lua, _) = create("demo:main", Execution::new("test", 123, "test")).unwrap();
    lua.load("math.randomseed(71); local a=math.random(); math.randomseed(71); assert(a==math.random()); assert(not pcall(math.random,0)); assert(not pcall(math.random,5,1))").exec().unwrap();
}

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tracing_subscriber::{Layer, layer::SubscriberExt};
type Events = Arc<Mutex<Vec<BTreeMap<String, String>>>>;
#[derive(Clone)]
struct Capture(Events);
struct Fields(BTreeMap<String, String>);
impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().into(), format!("{value:?}"));
    }
}
impl<S: tracing::Subscriber> Layer<S> for Capture {
    fn register_callsite(
        &self,
        _: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::always()
    }
    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(tracing::level_filters::LevelFilter::TRACE)
    }
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut fields = Fields(BTreeMap::new());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
}
fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<BTreeMap<String, String>>) {
    let events = Events::default();
    // Tracing's single-dispatch fast path consults the registering thread's
    // default. Parallel tests can first register our shared logging callsite
    // on a thread with no subscriber and cache `never`. Keep two dispatches
    // live so registration considers the scoped capture on every thread.
    let sentinel = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let subscriber = tracing_subscriber::registry().with(Capture(Arc::clone(&events)));
    let result = tracing::subscriber::with_default(subscriber, f);
    drop(sentinel);
    let records = events.lock().unwrap().clone();
    (result, records)
}
#[test]
fn failed_attempts_keep_diagnostics_and_helper_source_identity() {
    let (_, events) = capture(|| {
        let (lua, diagnostics) = create(
            "demo@1.0.0:main",
            Execution::new("gameplay", 123, "action:42"),
        )
        .unwrap();
        let helper: mlua::Function = lua
            .load("return function() log.debug('chosen',{recipe='jade',count=2,ready=true}) end")
            .set_name("tools@1.0.0:helper")
            .eval()
            .unwrap();
        let callback: mlua::Function=lua.load("return function(helper) helper(); print('attempt',3); error('failed after diagnostics') end")
            .set_name("demo@1.0.0:main").eval().unwrap();
        let error = callback.call::<()>(helper).unwrap_err();
        assert!(error.to_string().contains("failed after diagnostics"));
        diagnostics.finish("script_error");
    });
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(
        events[0]["module"].contains("tools@1.0.0:helper"),
        "{events:?}"
    );
    assert!(events[0]["package"].contains("demo@1.0.0"));
    assert!(events[0]["outcome"].contains("script_error"));
    assert!(events[0]["fields"].contains("jade") && events[0]["fields"].contains("true"));
    assert!(events[1]["message"].contains("attempt"));
}
#[test]
fn logging_pressure_does_not_change_random_results_and_retries_repeat_attempts() {
    let mut values = Vec::new();
    let (_, events) = capture(|| {
        for count in [0, 90, 90] {
            let (lua, diagnostics) =
                create("demo:main", Execution::new("test", 912, "action:stable")).unwrap();
            let callback: mlua::Function=lua.load("return function(count) for i=1,count do log.info('iteration',{iteration=i}) end; return math.random() end")
            .set_name("demo:main").eval().unwrap();
            values.push(callback.call::<f64>(count).unwrap());
            diagnostics.finish("evaluated");
        }
    });
    assert!(values.iter().all(|value| *value == values[0]));
    assert_eq!(events.len(), 2 * (diagnostics::MAX_RECORDS + 1));
    let first = &events[..diagnostics::MAX_RECORDS + 1];
    let second = &events[diagnostics::MAX_RECORDS + 1..];
    assert_eq!(first, second);
    assert!(first.last().unwrap().contains_key("suppressed"));
}
#[test]
fn diagnostic_encoding_rejects_nested_fields_without_executing_metamethods() {
    let (lua, diagnostics) = create("demo:main", Execution::new("test", 0, "test")).unwrap();
    lua.load(
        r#"
        assert(not pcall(function() log.info('bad',{nested={}}) end))
        assert(not pcall(function() log.info('bad',{n=0/0}) end))
        local touched=false
        print(setmetatable({}, {__tostring=function() touched=true; error('unexpected') end}))
        assert(not touched)
        log.warn('valid',{text='你好'})
        print(string.rep('你好',3000),string.char(255))
    "#,
    )
    .exec()
    .unwrap();
    diagnostics.finish("evaluated");
}
#[test]
fn common_runner_flushes_failed_invocations_and_cannot_hide_coroutine_budget_failure() {
    let (_, events) = capture(|| {
        let result = super::super::run_with(
            &super::super::Program::Source(super::super::SourceModule {
                id: "demo:failed".into(),
                source: "return function() log.error('before failure'); error('expected') end"
                    .into(),
            }),
            super::super::Limits::default(),
            Execution::new("presentation", 7, "sequence:7").client(),
            |_, entry| entry.call::<()>(()),
        );
        assert!(result.is_err());
    });
    assert_eq!(events.len(), 1);
    assert!(events[0]["outcome"].contains("script_error"));
    let result=super::super::run_with(&super::super::Program::Source(super::super::SourceModule {
        id:"demo:loop".into(), source:"return function() local co=coroutine.create(function() while true do end end); pcall(coroutine.resume,co); return 1 end".into(),
    }),super::super::Limits { max_interrupts:10, ..Default::default() }, Execution::new("test",0,"test"), |_,entry|entry.call::<i64>(()));
    assert!(matches!(
        result.unwrap_err().failure,
        super::super::ScriptFailure::InstructionLimit
    ));
}
