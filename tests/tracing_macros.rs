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
