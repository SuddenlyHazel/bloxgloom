use super::*;

fn module(id: &str, source: &str) -> SourceModule {
    SourceModule {
        id: id.into(),
        source: source.into(),
    }
}

fn input() -> ScriptInput {
    ScriptInput { tick: 8, seed: 13 }
}

#[test]
fn local_source_is_deterministic_and_runs_on_worker() {
    let worker = ScriptWorker::spawn(Limits::default()).unwrap();
    let source = "return function(input) return input.tick * 10 + input.seed end";
    assert_eq!(
        worker
            .execute(module("demo:calc", source), input())
            .unwrap(),
        93
    );
    assert_eq!(
        worker
            .execute(module("demo:calc", source), input())
            .unwrap(),
        93
    );

    // No VM state leaks across invocations, even for the same module.
    let source = "counter = (counter or 0) + 1; return function(_) return counter end";
    assert_eq!(
        worker
            .execute(module("demo:state", source), input())
            .unwrap(),
        1
    );
    assert_eq!(
        worker
            .execute(module("demo:state", source), input())
            .unwrap(),
        1
    );
}

#[test]
fn sandbox_excludes_native_io_and_attributes_errors() {
    let worker = ScriptWorker::spawn(Limits::default()).unwrap();
    let source = "return function(_) return (os.clock or os.time or os.date or io or require or loadfile or gcinfo) ~= nil and 1 or 0 end";
    assert_eq!(
        worker
            .execute(module("demo:sandbox", source), input())
            .unwrap(),
        0
    );

    let error = worker
        .execute(
            module("demo:broken", "return function(_) error('bad input') end"),
            input(),
        )
        .unwrap_err();
    assert_eq!(error.module, "demo:broken");
    assert!(matches!(error.failure, ScriptFailure::Lua(message) if message.contains("bad input")));
}

#[test]
fn instruction_and_source_limits_reject_bad_modules_without_poisoning_worker() {
    let limits = Limits {
        max_interrupts: 10,
        max_source_bytes: 100,
        ..Limits::default()
    };
    let worker = ScriptWorker::spawn(limits).unwrap();
    let error = worker
        .execute(
            module("demo:loop", "return function(_) while true do end end"),
            input(),
        )
        .unwrap_err();
    assert_eq!(error.module, "demo:loop");
    assert_eq!(error.failure, ScriptFailure::InstructionLimit);
    let error = worker
        .execute(module("demo:long", &"x".repeat(101)), input())
        .unwrap_err();
    assert_eq!(error.failure, ScriptFailure::SourceTooLarge);
    assert_eq!(
        worker
            .execute(
                module("demo:ok", "return function(_) return 2 end"),
                input()
            )
            .unwrap(),
        2
    );
}

#[test]
fn memory_limit_and_syntax_error_are_attributable() {
    let worker = ScriptWorker::spawn(Limits {
        max_memory_bytes: 256 * 1024,
        ..Limits::default()
    })
    .unwrap();
    let error = worker
        .execute(
            module(
                "demo:memory",
                "return function(_) return string.len(string.rep('x', 1000000)) end",
            ),
            input(),
        )
        .unwrap_err();
    assert_eq!(error.module, "demo:memory");
    assert!(matches!(error.failure, ScriptFailure::Lua(message) if message.contains("memory")));

    let error = worker
        .execute(module("demo:syntax", "return function("), input())
        .unwrap_err();
    assert_eq!(error.module, "demo:syntax");
    assert!(matches!(error.failure, ScriptFailure::Lua(_)));
}

#[test]
fn elapsed_deadline_is_reported_with_module_identity() {
    let worker = ScriptWorker::spawn(Limits {
        max_wall_time: Duration::from_nanos(1),
        ..Limits::default()
    })
    .unwrap();
    let error = worker
        .execute(
            module("demo:deadline", "return function(_) while true do end end"),
            input(),
        )
        .unwrap_err();
    assert_eq!(error.module, "demo:deadline");
    assert_eq!(error.failure, ScriptFailure::TimeLimit);
}
