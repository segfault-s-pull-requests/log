// Native tracing implementations of log's public macros.
//
// The macro bodies expand at the user's invocation site. In particular,
// tracing::event! expands its file!(), line!(), and module_path!() at that
// location, not in this file. Do not move emission into a normal Rust function.
//
// Compatibility boundaries (enabled only with feature "tracing-macros"):
//
// * Fixed target literals become real tracing callsite targets. Other target
//   expressions can be runtime strings, which tracing cannot put into static
//   metadata. They instead use the caller's module as target and are recorded
//   in a string field named log.target. This changes target-based filtering.
// * Dynamic levels use a match with one static tracing callsite per level.
//   This preserves log! Level expressions (evaluated once), but different
//   levels at one source location have different tracing callsite identities.
// * Explicit logger: arguments still use log's original backend. There is no
//   way to send a native tracing event to an arbitrary log::Log instance.
//   These are the only log! invocations that intentionally do not emit tracing.
// * The log::set_logger, log::logger, log::max_level and log::set_max_level
//   runtime APIs still exist, but do not affect native tracing emission.
//   Static log max_level_* feature filters DO remain in effect.
// * log KV fields are converted to tracing fields, retaining names and
//   primitive values. Unsupported types fall back to Debug. Serde/sval
//   structures are not recursively captured; they fall back to Debug.
//   Dynamic KV key *names* cannot become tracing callsite field names.
// * Tracing supports at most 32 fields per callsite, including message and
//   the possible log.target field.
// * The bridge does not preserve the possibility of using native log::Log
//   and tracing subscribers simultaneously for a single macro invocation.
//   Enabling tracing's optional "log" forwarding feature can also cause a
//   Cargo dependency cycle, since this fork of log depends on tracing.

/// Dispatch a log level to one of five compile-time tracing levels.
///
/// Keeping the event! invocation inside this macro (rather than in a function)
/// preserves the source-site file/line/module in tracing Metadata.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_emit {
    (target: $target:expr, level: $level:expr, fields: [$($field:tt)*], $($fmt:tt)+) => {{
        let __log_level = $level;
        if __log_level <= $crate::STATIC_MAX_LEVEL {
            match __log_level {
                $crate::Level::Error => $crate::__tracing::event!(
                    target: $target, $crate::__tracing::Level::ERROR,
                    { $($field)* }, $($fmt)+
                ),
                $crate::Level::Warn => $crate::__tracing::event!(
                    target: $target, $crate::__tracing::Level::WARN,
                    { $($field)* }, $($fmt)+
                ),
                $crate::Level::Info => $crate::__tracing::event!(
                    target: $target, $crate::__tracing::Level::INFO,
                    { $($field)* }, $($fmt)+
                ),
                $crate::Level::Debug => $crate::__tracing::event!(
                    target: $target, $crate::__tracing::Level::DEBUG,
                    { $($field)* }, $($fmt)+
                ),
                $crate::Level::Trace => $crate::__tracing::event!(
                    target: $target, $crate::__tracing::Level::TRACE,
                    { $($field)* }, $($fmt)+
                ),
            }
        }
    }};
}

/// Convert log KV grammar to tracing fields, keeping the raw formatting
/// tokens available for the tracing event! macro.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing {
    (target: $target:expr, $lvl:expr, fields: [$($extra:tt)*],
        $($key:tt $(:$capture:tt)? $(= $value:expr)?),+; $($fmt:tt)+
    ) => {
        $crate::__log_tracing_kv!(
            target: $target, $lvl, fields: [$($extra)*],
            $($key $(:$capture)? $(= $value)?),+; $($fmt)+
        )
    };
    (target: $target:expr, $lvl:expr, fields: [$($extra:tt)*], $($fmt:tt)+) => {
        $crate::__log_tracing_emit!(
            target: $target, level: $lvl, fields: [$($extra)*], $($fmt)+
        )
    };
}

/// KV shorthand and explicit values are processed by log's existing
/// __log_value! rules to preserve its ToValue, Debug, Display, error,
/// serde and sval type bounds and conditional feature errors.
#[cfg(feature = "kv")]
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_kv {
    (target: $target:expr, $lvl:expr, fields: [$($extra:tt)*],
        $($key:tt $(:$capture:tt)? $(= $value:expr)?),+; $($fmt:tt)+
    ) => {
        $crate::__log_tracing_emit!(
            target: $target, level: $lvl,
            fields: [
                $($extra)*
                $(
                    $key = $crate::__TracingKvValue(
                        $crate::__log_value!($key $(:$capture)? = $($value)?)
                    ),
                )+
            ],
            $($fmt)+
        )
    };
}

#[cfg(not(feature = "kv"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_kv {
    ($($tokens:tt)*) => {
        compile_error!("key value support requires the kv feature of log")
    };
}

/// Turns log::kv::Value into a native tracing::field::Value, retaining the
/// primitive type instead of eagerly stringifying all KV values.
#[cfg(feature = "kv")]
#[derive(Debug)]
pub struct __TracingKvValue<'v>(pub crate::kv::Value<'v>);

#[cfg(feature = "kv")]
impl crate::__tracing::field::Value for __TracingKvValue<'_> {
    fn record(&self, visitor: &mut dyn crate::__tracing::field::Visit) {
        struct Bridge<'a>(&'a mut dyn crate::__tracing::field::Visit);

        impl<'v> crate::kv::VisitValue<'v> for Bridge<'_> {
            fn visit_any(&mut self, value: crate::kv::Value<'v>) -> Result<(), crate::kv::Error> {
                self.0.record_debug(&value);
                Ok(())
            }

            fn visit_null(&mut self) -> Result<(), crate::kv::Error> {
                self.0.record_debug(&"null");
                Ok(())
            }

            fn visit_u64(&mut self, value: u64) -> Result<(), crate::kv::Error> {
                self.0.record_u64(value);
                Ok(())
            }

            fn visit_i64(&mut self, value: i64) -> Result<(), crate::kv::Error> {
                self.0.record_i64(value);
                Ok(())
            }

            fn visit_u128(&mut self, value: u128) -> Result<(), crate::kv::Error> {
                if let Ok(value) = u64::try_from(value) {
                    self.0.record_u64(value);
                } else {
                    self.0.record_debug(&value);
                }
                Ok(())
            }

            fn visit_i128(&mut self, value: i128) -> Result<(), crate::kv::Error> {
                if let Ok(value) = i64::try_from(value) {
                    self.0.record_i64(value);
                } else {
                    self.0.record_debug(&value);
                }
                Ok(())
            }

            fn visit_f64(&mut self, value: f64) -> Result<(), crate::kv::Error> {
                self.0.record_f64(value);
                Ok(())
            }

            fn visit_bool(&mut self, value: bool) -> Result<(), crate::kv::Error> {
                self.0.record_bool(value);
                Ok(())
            }

            fn visit_str(&mut self, value: &str) -> Result<(), crate::kv::Error> {
                self.0.record_str(value);
                Ok(())
            }

            fn visit_char(&mut self, value: char) -> Result<(), crate::kv::Error> {
                let mut bytes = [0; 4];
                self.0.record_str(value.encode_utf8(&mut bytes));
                Ok(())
            }
        }

        let _ = self.0.visit(Bridge(visitor));
    }
}

/// Log using a native tracing event. The logger: forms must use the original
/// log backend because a caller-provided logger cannot receive tracing events.
#[macro_export]
macro_rules! log {
    (logger: $logger:expr, target: $target:expr, $lvl:expr, $($arg:tt)+) => {
        $crate::__log!(
            logger: $crate::__log_logger!($logger),
            target: $target, $lvl, $($arg)+
        )
    };
    (logger: $logger:expr, $lvl:expr, $($arg:tt)+) => {
        $crate::__log!(
            logger: $crate::__log_logger!($logger),
            target: $crate::__private_api::module_path!(), $lvl, $($arg)+
        )
    };
    (target: $target:literal, $lvl:expr, $($arg:tt)+) => {
        $crate::__log_tracing!(
            target: $target, $lvl, fields: [], $($arg)+
        )
    };
    (target: $target:expr, $lvl:expr, $($arg:tt)+) => {
        $crate::__log_tracing!(
            target: $crate::__private_api::module_path!(), $lvl,
            fields: [log.target = %$target,], $($arg)+
        )
    };
    ($lvl:expr, $($arg:tt)+) => {
        $crate::__log_tracing!(
            target: $crate::__private_api::module_path!(), $lvl,
            fields: [], $($arg)+
        )
    };
}

/// Emits a ERROR-level log event through tracing (or the explicit logger).
#[macro_export]
macro_rules! error {
    (logger: $logger:expr, target: $target:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, target: $target, $crate::Level::Error, $($arg)+)
    };
    (logger: $logger:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, $crate::Level::Error, $($arg)+)
    };
    (target: $target:literal, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Error, $($arg)+)
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Error, $($arg)+)
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::Level::Error, $($arg)+)
    };
}

/// Emits a WARN-level log event through tracing (or the explicit logger).
#[macro_export]
macro_rules! warn {
    (logger: $logger:expr, target: $target:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, target: $target, $crate::Level::Warn, $($arg)+)
    };
    (logger: $logger:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, $crate::Level::Warn, $($arg)+)
    };
    (target: $target:literal, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Warn, $($arg)+)
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Warn, $($arg)+)
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::Level::Warn, $($arg)+)
    };
}

/// Emits a INFO-level log event through tracing (or the explicit logger).
#[macro_export]
macro_rules! info {
    (logger: $logger:expr, target: $target:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, target: $target, $crate::Level::Info, $($arg)+)
    };
    (logger: $logger:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, $crate::Level::Info, $($arg)+)
    };
    (target: $target:literal, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Info, $($arg)+)
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Info, $($arg)+)
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::Level::Info, $($arg)+)
    };
}

/// Emits a DEBUG-level log event through tracing (or the explicit logger).
#[macro_export]
macro_rules! debug {
    (logger: $logger:expr, target: $target:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, target: $target, $crate::Level::Debug, $($arg)+)
    };
    (logger: $logger:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, $crate::Level::Debug, $($arg)+)
    };
    (target: $target:literal, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Debug, $($arg)+)
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Debug, $($arg)+)
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::Level::Debug, $($arg)+)
    };
}

/// Emits a TRACE-level log event through tracing (or the explicit logger).
#[macro_export]
macro_rules! trace {
    (logger: $logger:expr, target: $target:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, target: $target, $crate::Level::Trace, $($arg)+)
    };
    (logger: $logger:expr, $($arg:tt)+) => {
        $crate::log!(logger: $logger, $crate::Level::Trace, $($arg)+)
    };
    (target: $target:literal, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Trace, $($arg)+)
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::log!(target: $target, $crate::Level::Trace, $($arg)+)
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::Level::Trace, $($arg)+)
    };
}

/// Check whether a native tracing event would be enabled at a log level.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_enabled {
    (target: $target:expr, $lvl:expr) => {{
        let __log_level = $lvl;
        if __log_level > $crate::STATIC_MAX_LEVEL {
            false
        } else {
            match __log_level {
                $crate::Level::Error => $crate::__tracing::event_enabled!(
                    target: $target, $crate::__tracing::Level::ERROR
                ),
                $crate::Level::Warn => $crate::__tracing::event_enabled!(
                    target: $target, $crate::__tracing::Level::WARN
                ),
                $crate::Level::Info => $crate::__tracing::event_enabled!(
                    target: $target, $crate::__tracing::Level::INFO
                ),
                $crate::Level::Debug => $crate::__tracing::event_enabled!(
                    target: $target, $crate::__tracing::Level::DEBUG
                ),
                $crate::Level::Trace => $crate::__tracing::event_enabled!(
                    target: $target, $crate::__tracing::Level::TRACE
                ),
            }
        }
    }};
}

/// With an explicit logger, retain log::Log::enabled semantics.
#[macro_export]
macro_rules! log_enabled {
    (logger: $logger:expr, target: $target:expr, $lvl:expr) => {
        $crate::__log_enabled!(
            logger: $crate::__log_logger!($logger), target: $target, $lvl
        )
    };
    (logger: $logger:expr, $lvl:expr) => {
        $crate::__log_enabled!(
            logger: $crate::__log_logger!($logger),
            target: $crate::__private_api::module_path!(), $lvl
        )
    };
    (target: $target:literal, $lvl:expr) => {
        $crate::__log_tracing_enabled!(target: $target, $lvl)
    };
    (target: $target:expr, $lvl:expr) => {{
        let __log_level = $lvl;
        if __log_level > $crate::STATIC_MAX_LEVEL {
            false
        } else {
            // Preserve evaluation of the caller's target expression, even
            // though tracing cannot filter on a runtime callsite target.
            let _ = &$target;
            $crate::__log_tracing_enabled!(
                target: $crate::__private_api::module_path!(), __log_level
            )
        }
    }};
    ($lvl:expr) => {
        $crate::__log_tracing_enabled!(
            target: $crate::__private_api::module_path!(), $lvl
        )
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn standard_invocations() {
        use crate::Level;
        let count = 42;
        let dynamic_target = "runtime-target";
        crate::error!("error {}", count);
        crate::warn!(target: "static-target", "warn {count}");
        crate::info!(target: dynamic_target, "info {}", count);
        crate::debug!(target: concat!("static", "-macro"), "debug {:?}", count);
        crate::trace!("trace");
        crate::log!(Level::Info, "dynamic {}", count);
        crate::log!(target: "static-target", Level::Info, "target");
        let _ = crate::log_enabled!(Level::Debug);
        let _ = crate::log_enabled!(target: "static-target", Level::Info);
    }

    // A Subscriber sees file/line/module from the invocation, not from the
    // macro definition. This also exercises emission through the dispatcher.
    #[cfg(feature = "std")]
    #[test]
    fn original_callsite_metadata() {
        use std::sync::{Arc, Mutex};
        use crate::__tracing::{self as tracing, span};

        struct Capture(Arc<Mutex<Vec<(String, u32, String, String)>>>);

        impl tracing::Subscriber for Capture {
            fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
                true
            }

            fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
                span::Id::from_u64(1)
            }

            fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

            fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

            fn event(&self, event: &tracing::Event<'_>) {
                let meta = event.metadata();
                self.0.lock().unwrap().push((
                    meta.file().unwrap_or("").to_owned(),
                    meta.line().unwrap_or(0),
                    meta.module_path().unwrap_or("").to_owned(),
                    meta.target().to_owned(),
                ));
            }

            fn enter(&self, _: &span::Id) {}

            fn exit(&self, _: &span::Id) {}
        }

        let records = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing::Dispatch::new(Capture(records.clone()));
        let expected_file = file!();
        let expected_module = module_path!();
        let expected_line = line!() + 2;
        tracing::dispatcher::with_default(&subscriber, || {
            crate::info!(target: "explicit-static-target", "caller location");
        });

        let records = records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, expected_file);
        assert_eq!(records[0].1, expected_line);
        assert_eq!(records[0].2, expected_module);
        assert_eq!(records[0].3, "explicit-static-target");
    }

    #[test]
    fn explicit_logger() {
        let logger = crate::__private_api::GlobalLogger;
        crate::warn!(logger: logger, "legacy logging");
        let _ = crate::log_enabled!(logger: logger, crate::Level::Warn);
    }

    #[cfg(feature = "kv")]
    #[test]
    fn key_value_invocations() {
        let count = 42_u64;
        let label = "hi";
        crate::info!(count, label:%; "values {}", count);
        crate::info!(target: "static", "named" = 123_u64, answer:? = count; "named");
        crate::info!(count:debug, label:display; "shorthand");
    }
}
