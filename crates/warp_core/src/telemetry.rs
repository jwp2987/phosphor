// Zap: telemetry sending has been physically removed. These macros remain as
// compatibility shims for call sites that still describe local UI/business events. They
// type-check the event expression in an unreachable branch without evaluating it, so
// telemetry-only payload construction has no runtime cost while callers keep their existing
// type inference and imports. Keep the context/executor operands referenced to avoid churn
// in callers while the remaining event-type shell is removed incrementally.

#[macro_export]
macro_rules! send_telemetry_from_ctx {
    ($event:expr_2021, $ctx:expr_2021) => {{
        if false {
            let _ = &$event;
        }
        let _ = &$ctx;
    }};
}

#[macro_export]
macro_rules! send_telemetry_from_app_ctx {
    ($event:expr_2021, $app_ctx:expr_2021) => {{
        if false {
            let _ = &$event;
        }
        let _ = &$app_ctx;
    }};
}

#[macro_export]
macro_rules! send_telemetry_sync_from_ctx {
    ($event:expr_2021, $ctx:expr_2021) => {{
        if false {
            let _ = &$event;
        }
        let _ = &$ctx;
    }};
}

#[macro_export]
macro_rules! send_telemetry_sync_from_app_ctx {
    ($event:expr_2021, $app_ctx:expr_2021) => {{
        if false {
            let _ = &$event;
        }
        let _ = &$app_ctx;
    }};
}

#[macro_export]
macro_rules! send_telemetry_on_executor {
    ($auth_state:expr_2021, $event:expr_2021, $executor:expr_2021) => {{
        if false {
            let _ = &$event;
        }
        let _ = &$auth_state;
        let _ = &$executor;
    }};
}
