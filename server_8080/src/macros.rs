#[macro_export]
#[clippy::format_args]
macro_rules! log_and {
    ($and:tt, "error", $($expr:tt)* $(,)?) => {{
        ::log::error!($($expr)*);
        $and;
    }};
    ($and:tt, "warn", $($expr:tt)* $(,)?) => {{
        ::log::warn!($($expr)*);
        $and;
    }};
    ($and:tt, "info", $($expr:tt)* $(,)?) => {{
        ::log::info!($($expr)*);
        $and;
    }};
    ($and:tt, "debug", $($expr:tt)* $(,)?) => {{
        ::log::debug!($($expr)*);
        $and;
    }};
    ($and:tt, "trace", $($expr:tt)* $(,)?) => {{
        ::log::trace!($($expr)*);
        $and;
    }};
}
