// Native tracing implementations of log's public macros.
//
// The macro bodies expand at the user's invocation site. In particular,
// this file constructs tracing-core events from statics whose file!(),
// line!(), and module_path!() expand at the caller's location. Do not move
// emission into a normal Rust function.
//
// Compatibility boundaries (enabled only with feature "tracing-macros"):
//
// * Literal and concat!(...) targets remain static tracing targets.
//   Other target expressions use the original log backend to avoid changing
//   target-based filtering; they get a NonLiteralTarget diagnostic entry.
// * Dynamic levels use a match with one static tracing callsite per level.
//   This preserves log! Level expressions (evaluated once), but different
//   levels at one source location have different tracing callsite identities.
// * Explicit logger: arguments still use log's original backend. There is no
//   way to send a native tracing event to an arbitrary log::Log instance.
//   Dynamic KV key names also fall back to log (with diagnostic records).
// * The log::set_logger, log::logger, log::max_level and log::set_max_level
//   runtime APIs still exist, but do not affect native tracing emission.
//   Static log max_level_* feature filters DO remain in effect.
// * log KV fields become tracing fields with the same names; values are
//   formatted via log::kv::Value (Display). Primitive types are therefore
//   not preserved as typed tracing values. Serde/sval structures are not
//   recursively captured.
//   Dynamic KV key names fall back to the log backend with a diagnostic entry.
// * Large callsite field sets may exceed subscriber or formatter limitations.
// * The bridge does not preserve the possibility of using native log::Log
//   and tracing subscribers simultaneously for a single macro invocation.
//   This implementation depends only on tracing-core (never on tracing).

/// Emit a diagnostic entry in the fallback-only table and execute the
/// original log backend. Every unsupported form that can be logged normally
/// must go through here rather than bypassing the diagnostic table.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_fallback {
    (logger: $logger:expr, target: $target:expr, level: $lvl:expr,
        reason: $reason:expr,
        $($key:tt $(:$capture:tt)? $(= $value:expr)?),+;
        $fmt:expr $(, $($fmt_arg:tt)*)?
    ) => {{
        static __LOG_FALLBACK_SOURCE: &str =
            $crate::__tracing_core::__tracing_capture_source!(
                []; $fmt $(, $($fmt_arg)*)?
            );
        $crate::__tracing_core::__tracing_record_log_fallback!(
            level: $lvl, target: $target, reason: $reason,
            template: $crate::__tracing_core::__macro_support::Option::Some($fmt),
            source: $crate::__tracing_core::__macro_support::Option::Some(
                __LOG_FALLBACK_SOURCE
            )
        );
        $crate::__log!(
            logger: $logger, target: $target, $lvl,
            $($key $(:$capture)? $(= $value)?),+;
            $fmt $(, $($fmt_arg)*)?
        )
    }};
    (logger: $logger:expr, target: $target:expr, level: $lvl:expr,
        reason: $reason:expr, $fmt:expr $(, $($fmt_arg:tt)*)?
    ) => {{
        static __LOG_FALLBACK_SOURCE: &str =
            $crate::__tracing_core::__tracing_capture_source!(
                []; $fmt $(, $($fmt_arg)*)?
            );
        $crate::__tracing_core::__tracing_record_log_fallback!(
            level: $lvl, target: $target, reason: $reason,
            template: $crate::__tracing_core::__macro_support::Option::Some($fmt),
            source: $crate::__tracing_core::__macro_support::Option::Some(
                __LOG_FALLBACK_SOURCE
            )
        );
        $crate::__log!(
            logger: $logger, target: $target, $lvl,
            $fmt $(, $($fmt_arg)*)?
        )
    }};
}

/// A KV field name must be a static identifier or string literal to become
/// tracing callsite metadata. The first dynamic name causes the *entire*
/// call to fall back to log; the original arguments remain intact.
#[cfg(feature = "kv")]
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_kv_scan {
    (
        emit: $emit:ident, target: $target:expr,
        emit_level: $emit_level:expr, log_level: $log_level:expr,
        fields: [$($field:tt)*], original: [$($original:tt)*],
        remaining: [], fmt: [$($fmt:tt)+]
    ) => {
        $crate::$emit!(
            target: $target, level: $emit_level,
            fields: [$($field)*], $($fmt)+
        )
    };
    (
        emit: $emit:ident, target: $target:expr,
        emit_level: $emit_level:expr, log_level: $log_level:expr,
        fields: [$($field:tt)*], original: [$($original:tt)*],
        remaining: [$key:ident $(:$capture:tt)? $(= $value:expr)?, $($rest:tt)*],
        fmt: [$($fmt:tt)+]
    ) => {
        $crate::__log_tracing_kv_scan!(
            emit: $emit, target: $target,
            emit_level: $emit_level, log_level: $log_level,
            fields: [
                $($field)*
                $key = %$crate::__log_value!($key $(:$capture)? = $($value)?),
            ],
            original: [$($original)*], remaining: [$($rest)*],
            fmt: [$($fmt)+]
        )
    };
    (
        emit: $emit:ident, target: $target:expr,
        emit_level: $emit_level:expr, log_level: $log_level:expr,
        fields: [$($field:tt)*], original: [$($original:tt)*],
        remaining: [$key:literal $(:$capture:tt)? $(= $value:expr)?, $($rest:tt)*],
        fmt: [$($fmt:tt)+]
    ) => {
        $crate::__log_tracing_kv_scan!(
            emit: $emit, target: $target,
            emit_level: $emit_level, log_level: $log_level,
            fields: [
                $($field)*
                $key = %$crate::__log_value!($key $(:$capture)? = $($value)?),
            ],
            original: [$($original)*], remaining: [$($rest)*],
            fmt: [$($fmt)+]
        )
    };
    (
        emit: $emit:ident, target: $target:expr,
        emit_level: $emit_level:expr, log_level: $log_level:expr,
        fields: [$($field:tt)*], original: [$($original:tt)*],
        remaining: [$key:tt $(:$capture:tt)? $(= $value:expr)?, $($rest:tt)*],
        fmt: [$($fmt:tt)+]
    ) => {
        $crate::__log_tracing_fallback!(
            logger: $crate::__log_logger!(__log_global_logger),
            target: $target, level: $log_level,
            reason: $crate::__tracing_core::callsite_table::LogFallbackReason::DynamicKey,
            $($original)*; $($fmt)+
        )
    };
}

/// Builds a real tracing-core callsite and emits an event through the active
/// dispatcher. Expanding in the caller's source (rather than a function) is
/// REQUIRED: file!(), line!(), and module_path!() must resolve there.
///
/// Only strings supplied as literal targets can be placed into the static
/// metadata. The fields are supplied as name = %Display expressions, with
/// "message" always the first entry in the field set. The format template
/// and recovered invocation source are exported through tracing-core as a
/// side record in the same special ELF section as tracing events.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_core_event {
    (target: $target:expr, level: $level:expr,
        fields: [$($key:tt = %$value:expr,)*],
        $fmt:expr $(, $($fmt_arg:tt)*)?
    ) => {{
        // Qualify Option variants so caller imports cannot shadow macro internals.
        static __META: $crate::__tracing_core::Metadata<'static> =
            $crate::__tracing_core::Metadata::new(
                concat!("event ", file!(), ":", line!()),
                $target,
                $level,
                $crate::__tracing_core::__macro_support::Option::Some(file!()),
                $crate::__tracing_core::__macro_support::Option::Some(line!()),
                $crate::__tracing_core::__macro_support::Option::Some(module_path!()),
                $crate::__tracing_core::field::FieldSet::new(
                    &["message", $($crate::__log_tracing_key!($key),)*],
                    $crate::__tracing_core::identify_callsite!(&__CALLSITE),
                ),
                $crate::__tracing_core::metadata::Kind::EVENT,
            );
        static __CALLSITE: $crate::__tracing_core::callsite::DefaultCallsite =
            $crate::__tracing_core::callsite::DefaultCallsite::new(&__META);
        // Share exactly the same linker record and source extractor used by
        // tracing::event!. Source extraction happens at this macro expansion
        // site, not in a function, preserving the outer invocation when Rust
        // exposes the necessary span ancestry.
        static __TRACING_SOURCE: &str =
            $crate::__tracing_core::__tracing_capture_source!([]; $fmt $(, $($fmt_arg)*)?);
        $crate::__tracing_core::__tracing_record_metadata!(
            __META;
            $crate::__tracing_core::__macro_support::Option::Some($fmt);
            $crate::__tracing_core::__macro_support::Option::Some(__TRACING_SOURCE)
        );
        if $level <= $crate::__tracing_core::LevelFilter::current() {
            let __interest = __CALLSITE.interest();
            if !__interest.is_never()
                && (__interest.is_always()
                    || $crate::__tracing_core::dispatcher::get_default(
                        |dispatch| dispatch.enabled(&__META)
                    ))
            {
                // An immediately invoked closure keeps temporaries in
                // format_args! (and KV expressions) alive through dispatch.
                (|__fields: $crate::__tracing_core::field::ValueSet<'_>| {
                    $crate::__tracing_core::Event::dispatch(&__META, &__fields);
                })(__META.fields().value_set_all(&[
                    $crate::__tracing_core::__macro_support::Option::Some(
                        &format_args!($fmt $(, $($fmt_arg)*)?)
                            as &dyn $crate::__tracing_core::field::Value
                    ),
                    $(
                        $crate::__tracing_core::__macro_support::Option::Some(
                            &$crate::__tracing_core::field::display(&$value)
                                as &dyn $crate::__tracing_core::field::Value
                        ),
                    )*
                ]));
            }
        }
    }};
}

/// Compile-time metadata names for user KV fields; log permits runtime key
/// expressions, but tracing-core's callsite names must be static.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_key {
    ($key:ident) => { stringify!($key) };
    ($key:literal) => { $key };
    ($key:tt) => { stringify!($key) };
}

/// Dispatch runtime log level expressions (evaluated exactly once) to the
/// five tracing-core levels. Distinct levels have distinct static callsites.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_emit {
    (target: $target:expr, level: $level:expr, fields: [$($field:tt)*], $($fmt:tt)+) => {{
        let __log_level = $level;
        if __log_level <= $crate::STATIC_MAX_LEVEL {
            match __log_level {
                $crate::Level::Error => $crate::__log_tracing_core_event!(
                    target: $target, level: $crate::__tracing_core::Level::ERROR,
                    fields: [$($field)*], $($fmt)+
                ),
                $crate::Level::Warn => $crate::__log_tracing_core_event!(
                    target: $target, level: $crate::__tracing_core::Level::WARN,
                    fields: [$($field)*], $($fmt)+
                ),
                $crate::Level::Info => $crate::__log_tracing_core_event!(
                    target: $target, level: $crate::__tracing_core::Level::INFO,
                    fields: [$($field)*], $($fmt)+
                ),
                $crate::Level::Debug => $crate::__log_tracing_core_event!(
                    target: $target, level: $crate::__tracing_core::Level::DEBUG,
                    fields: [$($field)*], $($fmt)+
                ),
                $crate::Level::Trace => $crate::__log_tracing_core_event!(
                    target: $target, level: $crate::__tracing_core::Level::TRACE,
                    fields: [$($field)*], $($fmt)+
                ),
            }
        }
    }};
}

/// Emit a single static callsite for log's fixed-severity macros.
///
/// This is separate from the dynamic log! dispatcher because each of the
/// five possible runtime levels requires its own static Metadata. A known
/// severity must NOT generate five redundant records in the linker section.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_fixed {
    (target: $target:expr, log_level: $log_lvl:expr, tracing_level: $tracing_lvl:expr,
        fields: [$($extra:tt)*],
        $($key:tt $(:$capture:tt)? $(= $value:expr)?),+; $($fmt:tt)+
    ) => {
        $crate::__log_tracing_fixed_kv!(
            target: $target, log_level: $log_lvl, tracing_level: $tracing_lvl,
            fields: [$($extra)*], $($key $(:$capture)? $(= $value)?),+; $($fmt)+
        )
    };
    (target: $target:expr, log_level: $log_lvl:expr, tracing_level: $tracing_lvl:expr,
        fields: [$($extra:tt)*], $($fmt:tt)+
    ) => {{
        if $log_lvl <= $crate::STATIC_MAX_LEVEL {
            $crate::__log_tracing_core_event!(
                target: $target, level: $tracing_lvl,
                fields: [$($extra)*], $($fmt)+
            )
        }
    }};
}

#[cfg(feature = "kv")]
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_fixed_kv {
    (target: $target:expr, log_level: $log_lvl:expr, tracing_level: $tracing_lvl:expr,
        fields: [$($extra:tt)*],
        $($key:tt $(:$capture:tt)? $(= $value:expr)?),+; $($fmt:tt)+
    ) => {{
        if $log_lvl <= $crate::STATIC_MAX_LEVEL {
            $crate::__log_tracing_kv_scan!(
                emit: __log_tracing_core_event,
                target: $target, emit_level: $tracing_lvl, log_level: $log_lvl,
                fields: [$($extra)*],
                original: [$($key $(:$capture)? $(= $value)?),+],
                remaining: [$($key $(:$capture)? $(= $value)?,)+],
                fmt: [$($fmt)+]
            )
        }
    }};
}

#[cfg(not(feature = "kv"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_fixed_kv {
    ($($tokens:tt)*) => {
        compile_error!("key value support requires the kv feature of log")
    };
}

/// Convert log KV grammar to tracing fields, keeping the raw formatting
/// tokens available for format_args! at the original callsite.
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
        $crate::__log_tracing_kv_scan!(
            emit: __log_tracing_emit,
            target: $target, emit_level: $lvl, log_level: $lvl,
            fields: [$($extra)*],
            original: [$($key $(:$capture)? $(= $value)?),+],
            remaining: [$($key $(:$capture)? $(= $value)?,)+],
            fmt: [$($fmt)+]
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

/// Log using a native tracing event. The logger: forms must use the original
/// log backend because a caller-provided logger cannot receive tracing events.
#[macro_export]
macro_rules! log {
    (logger: $logger:expr, target: $target:expr, $lvl:expr, $($arg:tt)+) => {
        $crate::__log_tracing_fallback!(
            logger: $crate::__log_logger!($logger),
            target: $target, level: $lvl,
            reason: $crate::__tracing_core::callsite_table::LogFallbackReason::ExplicitLogger,
            $($arg)+
        )
    };
    (logger: $logger:expr, $lvl:expr, $($arg:tt)+) => {
        $crate::__log_tracing_fallback!(
            logger: $crate::__log_logger!($logger),
            target: $crate::__private_api::module_path!(), level: $lvl,
            reason: $crate::__tracing_core::callsite_table::LogFallbackReason::ExplicitLogger,
            $($arg)+
        )
    };
    (target: $target:literal, $lvl:expr, $($arg:tt)+) => {
        $crate::__log_tracing!(
            target: $target, $lvl, fields: [], $($arg)+
        )
    };
    (target: concat!($($target_parts:tt)*), $lvl:expr, $($arg:tt)+) => {
        $crate::__log_tracing!(
            target: concat!($($target_parts)*), $lvl, fields: [], $($arg)+
        )
    };
    (target: $target:expr, $lvl:expr, $($arg:tt)+) => {
        $crate::__log_tracing_fallback!(
            logger: $crate::__log_logger!(__log_global_logger),
            target: $target, level: $lvl,
            reason: $crate::__tracing_core::callsite_table::LogFallbackReason::NonLiteralTarget,
            $($arg)+
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
        $crate::__log_tracing_fixed!(
            target: $target, log_level: $crate::Level::Error,
            tracing_level: $crate::__tracing_core::Level::ERROR,
            fields: [], $($arg)+
        )
    };
    (target: concat!($($target_parts:tt)*), $($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: concat!($($target_parts)*),
            log_level: $crate::Level::Error,
            tracing_level: $crate::__tracing_core::Level::ERROR,
            fields: [], $($arg)+
        )
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::__log_tracing_fallback!(
            logger: $crate::__log_logger!(__log_global_logger),
            target: $target, level: $crate::Level::Error,
            reason: $crate::__tracing_core::callsite_table::LogFallbackReason::NonLiteralTarget,
            $($arg)+
        )
    };
    ($($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: $crate::__private_api::module_path!(),
            log_level: $crate::Level::Error,
            tracing_level: $crate::__tracing_core::Level::ERROR,
            fields: [], $($arg)+
        )
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
        $crate::__log_tracing_fixed!(
            target: $target, log_level: $crate::Level::Warn,
            tracing_level: $crate::__tracing_core::Level::WARN,
            fields: [], $($arg)+
        )
    };
    (target: concat!($($target_parts:tt)*), $($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: concat!($($target_parts)*),
            log_level: $crate::Level::Warn,
            tracing_level: $crate::__tracing_core::Level::WARN,
            fields: [], $($arg)+
        )
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::__log_tracing_fallback!(
            logger: $crate::__log_logger!(__log_global_logger),
            target: $target, level: $crate::Level::Warn,
            reason: $crate::__tracing_core::callsite_table::LogFallbackReason::NonLiteralTarget,
            $($arg)+
        )
    };
    ($($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: $crate::__private_api::module_path!(),
            log_level: $crate::Level::Warn,
            tracing_level: $crate::__tracing_core::Level::WARN,
            fields: [], $($arg)+
        )
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
        $crate::__log_tracing_fixed!(
            target: $target, log_level: $crate::Level::Info,
            tracing_level: $crate::__tracing_core::Level::INFO,
            fields: [], $($arg)+
        )
    };
    (target: concat!($($target_parts:tt)*), $($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: concat!($($target_parts)*),
            log_level: $crate::Level::Info,
            tracing_level: $crate::__tracing_core::Level::INFO,
            fields: [], $($arg)+
        )
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::__log_tracing_fallback!(
            logger: $crate::__log_logger!(__log_global_logger),
            target: $target, level: $crate::Level::Info,
            reason: $crate::__tracing_core::callsite_table::LogFallbackReason::NonLiteralTarget,
            $($arg)+
        )
    };
    ($($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: $crate::__private_api::module_path!(),
            log_level: $crate::Level::Info,
            tracing_level: $crate::__tracing_core::Level::INFO,
            fields: [], $($arg)+
        )
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
        $crate::__log_tracing_fixed!(
            target: $target, log_level: $crate::Level::Debug,
            tracing_level: $crate::__tracing_core::Level::DEBUG,
            fields: [], $($arg)+
        )
    };
    (target: concat!($($target_parts:tt)*), $($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: concat!($($target_parts)*),
            log_level: $crate::Level::Debug,
            tracing_level: $crate::__tracing_core::Level::DEBUG,
            fields: [], $($arg)+
        )
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::__log_tracing_fallback!(
            logger: $crate::__log_logger!(__log_global_logger),
            target: $target, level: $crate::Level::Debug,
            reason: $crate::__tracing_core::callsite_table::LogFallbackReason::NonLiteralTarget,
            $($arg)+
        )
    };
    ($($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: $crate::__private_api::module_path!(),
            log_level: $crate::Level::Debug,
            tracing_level: $crate::__tracing_core::Level::DEBUG,
            fields: [], $($arg)+
        )
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
        $crate::__log_tracing_fixed!(
            target: $target, log_level: $crate::Level::Trace,
            tracing_level: $crate::__tracing_core::Level::TRACE,
            fields: [], $($arg)+
        )
    };
    (target: concat!($($target_parts:tt)*), $($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: concat!($($target_parts)*),
            log_level: $crate::Level::Trace,
            tracing_level: $crate::__tracing_core::Level::TRACE,
            fields: [], $($arg)+
        )
    };
    (target: $target:expr, $($arg:tt)+) => {
        $crate::__log_tracing_fallback!(
            logger: $crate::__log_logger!(__log_global_logger),
            target: $target, level: $crate::Level::Trace,
            reason: $crate::__tracing_core::callsite_table::LogFallbackReason::NonLiteralTarget,
            $($arg)+
        )
    };
    ($($arg:tt)+) => {
        $crate::__log_tracing_fixed!(
            target: $crate::__private_api::module_path!(),
            log_level: $crate::Level::Trace,
            tracing_level: $crate::__tracing_core::Level::TRACE,
            fields: [], $($arg)+
        )
    };
}

/// Static callsite with EVENT metadata, used by log_enabled!.
/// As with tracing::event_enabled!, this is a hint and not a promise that an
/// event will be accepted by subscriber-side value filtering.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_core_enabled {
    (target: $target:expr, level: $level:expr) => {{
        // Qualify Option variants so caller imports cannot shadow macro internals.
        static __META: $crate::__tracing_core::Metadata<'static> =
            $crate::__tracing_core::Metadata::new(
                concat!("enabled ", file!(), ":", line!()),
                $target,
                $level,
                $crate::__tracing_core::__macro_support::Option::Some(file!()),
                $crate::__tracing_core::__macro_support::Option::Some(line!()),
                $crate::__tracing_core::__macro_support::Option::Some(module_path!()),
                $crate::__tracing_core::field::FieldSet::new(
                    &["message"],
                    $crate::__tracing_core::identify_callsite!(&__CALLSITE),
                ),
                $crate::__tracing_core::metadata::Kind::EVENT,
            );
        static __CALLSITE: $crate::__tracing_core::callsite::DefaultCallsite =
            $crate::__tracing_core::callsite::DefaultCallsite::new(&__META);
        // This creates a filtering-only callsite without adding it to the
        // recording table: log_enabled! queries the subscriber but emits no event.
        if $level <= $crate::__tracing_core::LevelFilter::current() {
            let __interest = __CALLSITE.interest();
            !__interest.is_never()
                && (__interest.is_always()
                    || $crate::__tracing_core::dispatcher::get_default(
                        |dispatch| dispatch.enabled(&__META)
                    ))
        } else {
            false
        }
    }};
}

/// Tests whether a tracing event may be enabled at this source callsite.
#[doc(hidden)]
#[macro_export]
macro_rules! __log_tracing_enabled {
    (target: $target:expr, $lvl:expr) => {{
        let __log_level = $lvl;
        if __log_level > $crate::STATIC_MAX_LEVEL {
            false
        } else {
            match __log_level {
                $crate::Level::Error => $crate::__log_tracing_core_enabled!(
                    target: $target, level: $crate::__tracing_core::Level::ERROR
                ),
                $crate::Level::Warn => $crate::__log_tracing_core_enabled!(
                    target: $target, level: $crate::__tracing_core::Level::WARN
                ),
                $crate::Level::Info => $crate::__log_tracing_core_enabled!(
                    target: $target, level: $crate::__tracing_core::Level::INFO
                ),
                $crate::Level::Debug => $crate::__log_tracing_core_enabled!(
                    target: $target, level: $crate::__tracing_core::Level::DEBUG
                ),
                $crate::Level::Trace => $crate::__log_tracing_core_enabled!(
                    target: $target, level: $crate::__tracing_core::Level::TRACE
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
    (target: concat!($($target_parts:tt)*), $lvl:expr) => {
        $crate::__log_tracing_enabled!(
            target: concat!($($target_parts)*), $lvl
        )
    };
    // A runtime target cannot be queried against static tracing metadata.
    // Delegate to the original log enabled query, without an event record:
    // log_enabled! is a query, not a log event.
    (target: $target:expr, $lvl:expr) => {
        $crate::__log_enabled!(
            logger: $crate::__log_logger!(__log_global_logger),
            target: $target, $lvl
        )
    };
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
    // Verify that shimmed log callsites appear in the exact same section
    // read by tracing::callsite_table::records(). Each dynamic log level
    // currently contributes an independent (otherwise equivalent) record.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn linked_callsite_has_template_and_source() {
        let expected_file = file!();
        let expected_line = line!() + 1;
        crate::info!("shim template {}", 2718);

        let records: Vec<_> = crate::__tracing_core::callsite_table::records()
            .filter(|r| {
                let Some(metadata) = r.metadata() else { return false; };
                metadata.file() == Some(expected_file)
                    && metadata.line() == Some(expected_line)
            })
            .collect();
        assert_eq!(records.len(), 1, "a fixed-severity log callsite must have one ELF record");
        for record in records {
            assert_eq!(record.template(), Some("shim template {}"));
            let source = record.source().expect("source text in linker record");
            assert!(!source.is_empty(), "source text must not be empty");
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn original_callsite_metadata() {
        use std::sync::{Arc, Mutex};
        use crate::__tracing_core::{self as tracing, span};

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
