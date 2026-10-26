//! Compatibility matrix for the feature-gated native tracing replacement.
//! This intentionally exercises the public macros exactly as an application
//! would, including upstream log's special KV and explicit-logger grammar.
#![cfg(feature = "tracing-macros")]

use log::{debug, error, info, log, log_enabled, trace, warn, Level};

#[test]
fn upstream_standard_macro_forms() {
    let count = 42_u32;
    let who = "person";
    error!("error");
    warn!("warn {}", count);
    info!("literal and named captures {count} {who}");
    debug!("debug {:04x} {:?}", count, (1, 2));
    trace!("trace {} {}", count, who);

    info!(target: "the_static_target", "static {}", who);

    let target = String::from("runtime-target");
    info!(target: target.as_str(), "dynamic {}", count);
    debug!(target: concat!("const", "-target"), "non-literal constant");

    log!(Level::Error, "dynamic {}", count);
    log!(Level::Warn, "warn");
    log!(Level::Info, "info");
    log!(Level::Debug, "debug");
    log!(Level::Trace, "trace");
    log!(target: "static_log_target", Level::Info, "target {who}");
    log!(target: target.as_str(), Level::Debug, "runtime target {}", count);

    let _ = log_enabled!(Level::Trace);
    let _ = log_enabled!(Level::Error);
    let _ = log_enabled!(target: "static_target", Level::Info);
    let _ = log_enabled!(target: target.as_str(), Level::Debug);
}

#[test]
fn original_logger_grammar_compiles() {
    struct Logger;
    impl log::Log for Logger {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool { true }
        fn log(&self, _: &log::Record<'_>) {}
        fn flush(&self) {}
    }

    let logger = Logger;
    error!(logger: logger, "explicit error");
    warn!(logger: logger, target: "logger-target", "explicit warn {}", 13);
    info!(logger: logger, "explicit info");
    debug!(logger: logger, "explicit debug");
    trace!(logger: logger, "explicit trace");
    log!(logger: logger, Level::Info, "log dynamic level");
    log!(logger: logger, target: "target", Level::Warn, "log target");
    let _ = log_enabled!(logger: logger, Level::Warn);
    let _ = log_enabled!(logger: logger, target: "logger-target", Level::Info);
}

#[test]
fn explicit_logger_bypasses_tracing_and_is_diagnosable() {
    struct Logger(std::sync::atomic::AtomicUsize);
    impl log::Log for Logger {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool { true }
        fn log(&self, _: &log::Record<'_>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        fn flush(&self) {}
    }

    let logger = Logger(std::sync::atomic::AtomicUsize::new(0));
    // A custom logger's calls still pass through log's static AND global
    // max-level filters, as log's original explicit-logger macro does.
    log::set_max_level(log::LevelFilter::Trace);
    let expected_line = line!() + 1;
    warn!(logger: logger, "explicit fallback {}", 1234_u32);
    assert_eq!(logger.0.load(std::sync::atomic::Ordering::Relaxed), 1);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use log::__tracing_core::callsite_table::{
            log_fallback_records, LogFallbackReason,
        };
        let matching: Vec<_> = log_fallback_records()
            .filter(|r| r.file() == Some(file!()) && r.line() == expected_line)
            .collect();
        assert_eq!(matching.len(), 1, "one fallback record per logger callsite");
        assert_eq!(matching[0].reason(), LogFallbackReason::ExplicitLogger);
        assert_eq!(matching[0].template(), Some("explicit fallback {}"));
        assert_eq!(matching[0].module_path(), module_path!());
        assert!(!matching[0].source().unwrap_or("").is_empty());

        let native_count = log::__tracing_core::callsite_table::records()
            .filter(|r| r.metadata().unwrap().file() == Some(file!())
                && r.metadata().unwrap().line() == Some(expected_line))
            .count();
        assert_eq!(native_count, 0, "explicit logger must not create native metadata");
    }
}

#[test]
fn nonliteral_target_uses_log_backend_and_records_reason() {
    let target = String::from("runtime-target");
    let expected_line = line!() + 1;
    info!(target: target.as_str(), "dynamic target {}", 17);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use log::__tracing_core::callsite_table::{
            log_fallback_records, LogFallbackReason,
        };
        let entries: Vec<_> = log_fallback_records()
            .filter(|e| e.file() == Some(file!()) && e.line() == expected_line)
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].reason(), LogFallbackReason::NonLiteralTarget);
        assert_eq!(entries[0].template(), Some("dynamic target {}"));
        assert!(entries[0].target_expression().contains("target"));
        assert!(!entries[0].source().unwrap_or("").is_empty());
        assert_eq!(
            log::__tracing_core::callsite_table::records()
                .filter(|r| r.metadata().unwrap().file() == Some(file!())
                    && r.metadata().unwrap().line() == Some(expected_line))
                .count(),
            0
        );
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn concat_target_is_a_real_tracing_target() {
    let expected_line = line!() + 1;
    info!(target: concat!("static", "-concat"), "concat target {}", 27);
    let native: Vec<_> = log::__tracing_core::callsite_table::records()
        .filter(|r| r.metadata().unwrap().file() == Some(file!())
            && r.metadata().unwrap().line() == Some(expected_line))
        .collect();
    assert_eq!(native.len(), 1);
    assert_eq!(native[0].metadata().unwrap().target(), "static-concat");
    assert_eq!(native[0].template(), Some("concat target {}"));
}

#[cfg(feature = "kv")]
#[test]
fn static_structured_fields_emit_native_events() {
    let number = 15_u64;
    let label = "hello";
    info!(number, label:%; "static KV {}", number);
    warn!(target: "named", name:? = label, "other" = number; "static key");
    debug!(number:debug, label:display; "captures");
}

#[cfg(feature = "kv")]
#[test]
fn dynamic_structured_key_falls_back_without_renaming() {
    let runtime_name = "runtime-key";
    let expected_line = line!() + 1;
    info!((runtime_name) = 2718_u64; "dynamic key {}", 2718);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use log::__tracing_core::callsite_table::{
            log_fallback_records, LogFallbackReason,
        };
        let matching: Vec<_> = log_fallback_records()
            .filter(|r| r.file() == Some(file!()) && r.line() == expected_line)
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].reason(), LogFallbackReason::DynamicKey);
        assert_eq!(matching[0].template(), Some("dynamic key {}"));
        assert!(!matching[0].source().unwrap_or("").is_empty());
        let native_count = log::__tracing_core::callsite_table::records()
            .filter(|r| r.metadata().unwrap().file() == Some(file!())
                && r.metadata().unwrap().line() == Some(expected_line))
            .count();
        assert_eq!(native_count, 0);
    }
}

#[cfg(feature = "kv")]
#[test]
fn explicit_logger_and_kv_preserves_original_backend() {
    struct Logger;
    impl log::Log for Logger {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool { true }
        fn log(&self, record: &log::Record<'_>) {
            let key = record.key_values().get(log::kv::Key::from_str("key")).unwrap();
            assert_eq!(key.to_string(), "17");
        }
        fn flush(&self) {}
    }
    let logger = Logger;
    info!(logger: logger, key = 17; "with kv {}", 42);
}

// Unlike a text-only subscriber, this visitor fails if a native message falls
// back to fmt::Arguments/record_debug. No formatter runs before visit_format.
#[cfg(feature = "std")]
mod inspectable {
    use super::*;
    use log::__tracing_core::{
        self as core,
        field::{Field, Format, Visit},
        span, Event, Metadata, Subscriber,
    };
    use std::{
        fmt,
        sync::{Arc, Mutex},
    };

    #[derive(Debug, PartialEq)]
    struct Message {
        level: core::Level,
        template: &'static str,
        types: Vec<&'static str>,
        text: String,
    }

    #[derive(Clone)]
    struct Inspect {
        enabled: bool,
        messages: Arc<Mutex<Vec<Message>>>,
    }

    impl Visit for Inspect {
        fn visit_format(&mut self, field: &Field, value: &Format<'_>) {
            assert_eq!(field.name(), "message");
            // The event fills in its level after recording the borrowed view.
            self.messages.lock().unwrap().push(Message {
                level: core::Level::INFO,
                template: value.template(),
                types: value
                    .args()
                    .iter()
                    .map(|arg| arg.value().type_name())
                    .collect(),
                text: value.render().unwrap(),
            });
        }

        fn record_debug(&mut self, field: &Field, _: &dyn fmt::Debug) {
            assert_ne!(
                field.name(),
                "message",
                "native messages must use visit_format"
            );
        }
    }

    impl Subscriber for Inspect {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            self.enabled
        }
        fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }
        fn event(&self, event: &Event<'_>) {
            event.record(&mut self.clone());
            self.messages.lock().unwrap().last_mut().unwrap().level = *event.metadata().level();
        }
        fn record(&self, _: &span::Id, _: &span::Record<'_>) {}
        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}
        fn enter(&self, _: &span::Id) {}
        fn exit(&self, _: &span::Id) {}
    }

    #[test]
    fn every_level_delivers_typed_formats_and_keeps_temporary_owners_alive() {
        let subscriber = Inspect {
            enabled: true,
            messages: Arc::default(),
        };
        core::dispatcher::with_default(&core::Dispatch::new(subscriber.clone()), || {
            let number = 31u32;
            error!("error {number}");
            warn!("warn {}", number);
            info!("literal");
            debug!("debug {number:#06x}");
            trace!("trace {}", String::from("temporary").as_str());
            // Runtime levels, template macros, and named width operands use
            // the same representation as the fixed-severity public macros.
            for level in [
                Level::Error,
                Level::Warn,
                Level::Info,
                Level::Debug,
                Level::Trace,
            ] {
                log!(
                    level,
                    concat!("dynamic ", "{number:>width$}"),
                    number = number,
                    width = 4usize
                );
            }
        });
        let messages = subscriber.messages.lock().unwrap();
        assert_eq!(messages.len(), 10);
        assert_eq!(messages[0].types, ["u32"]);
        assert_eq!(messages[1].types, ["u32"]);
        assert!(messages[2].types.is_empty());
        assert_eq!(messages[3].types, ["u32"]);
        assert_eq!(messages[3].text, "debug 0x001f");
        assert_eq!(messages[4].types, ["str"]);
        assert_eq!(messages[4].text, "trace temporary");
        for (message, level) in messages[5..].iter().zip([
            core::Level::ERROR,
            core::Level::WARN,
            core::Level::INFO,
            core::Level::DEBUG,
            core::Level::TRACE,
        ]) {
            assert_eq!(message.level, level);
            assert_eq!(message.template, "dynamic {number:>width$}");
            assert_eq!(message.types, ["u32", "usize"]);
            assert_eq!(message.text, "dynamic   31");
        }
    }

    #[test]
    fn disabled_events_do_not_evaluate_message_arguments() {
        let subscriber = Inspect {
            enabled: false,
            messages: Arc::default(),
        };
        core::dispatcher::with_default(&core::Dispatch::new(subscriber.clone()), || {
            let argument = || -> u32 { panic!("disabled message argument evaluated") };
            debug!("disabled {}", argument());
            log!(Level::Trace, "disabled {}", argument());
        });
        assert!(subscriber.messages.lock().unwrap().is_empty());
    }

    #[cfg(feature = "kv")]
    #[test]
    fn kv_expressions_keep_evaluation_order_and_caller_control_flow() {
        use std::cell::RefCell;

        // KV expressions must not migrate into inspect_format's callback,
        // where return and ? would target the callback instead of this caller.
        fn emit(result: Result<u32, ()>) -> Result<(), ()> {
            info!(number = result?; "question {}", 1u32);
            info!(number = {
                if result == Ok(7) { return Ok(()); }
                9u32
            }; "return {}", 2u32);
            Err(())
        }

        let subscriber = Inspect {
            enabled: true,
            messages: Arc::default(),
        };
        core::dispatcher::with_default(&core::Dispatch::new(subscriber.clone()), || {
            let order = RefCell::new(Vec::new());
            let mark = |name, value| {
                order.borrow_mut().push(name);
                value
            };
            info!(number = mark("kv", 7u32); "message {}", mark("message", 31u32));
            assert_eq!(*order.borrow(), ["message", "kv"]);
            assert_eq!(emit(Err(())), Err(()));
            assert_eq!(emit(Ok(7)), Ok(()));
        });
        let messages = subscriber.messages.lock().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].types, ["u32"]);
        assert_eq!(messages[0].text, "message 31");
        assert_eq!(messages[1].text, "question 1");
    }
}
