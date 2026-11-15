use clap::ValueEnum;

#[derive(ValueEnum, Clone, Copy)]
pub enum ImgFmt {
    Png,
    #[cfg(any(feature = "svg-latex", feature = "svg-typst"))]
    Svg,
}

impl ImgFmt {
    pub fn name(self) -> String {
        value_name(&self)
    }
}

pub fn value_names<T: ValueEnum + 'static>() -> impl Iterator<Item = String> {
    T::value_variants()
        .iter()
        .filter_map(|v| v.to_possible_value())
        .map(|p| p.get_name().to_owned())
}

pub fn value_name<T: ValueEnum>(v: &T) -> String {
    v.to_possible_value()
        .map(|p| p.get_name().to_owned())
        .unwrap_or_default()
}

#[macro_export]
macro_rules! styled_eprintln {
    ($color:expr, $tag:literal, $($arg:tt)*) => {{
        let s = $color.on_default();
        eprintln!("{s}[{}]: {}{s:#}", $tag, format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::styled_eprintln!(
            clap::builder::styling::AnsiColor::Yellow, "WARN", $($arg)*
        )
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::styled_eprintln!(
            clap::builder::styling::AnsiColor::Red, "ERR", $($arg)*
        )
    };
}

#[macro_export]
macro_rules! watch_log {
    ($($arg:tt)*) => {
        eprintln!("[WATCH]: {}", format!($($arg)*))
    }
}
