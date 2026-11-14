#[cfg(not(any(feature = "latex", feature = "typst")))]
compile_error!("Enable at least one of latex or typst features");

#[cfg(feature = "latex")]
mod latex;
#[cfg(feature = "typst")]
mod typst;
mod utils;
mod consts;

use std::sync::LazyLock;

use clap::{Parser, ValueEnum};

#[cfg(feature = "latex")]
use crate::latex::render_latex;
#[cfg(feature = "typst")]
use crate::typst::render_typst;

#[derive(clap::ValueEnum, Clone, Copy)]
enum Format {
    #[cfg(feature = "latex")]
    Latex,
    #[cfg(feature = "typst")]
    Typst,
}

impl Format {
    // Function implementation taken from tylax
    #[cfg(all(feature = "latex", feature = "typst"))]
    pub fn detect_fmt(s: &str) -> Option<Self> {
        let latex_score: i32 = if s.contains("\\documentclass") { 10 } else { 0 }
            + if s.contains("\\begin{document}") {
                10
            } else {
                0
            }
            + if s.contains("\\section") { 5 } else { 0 }
            + if s.contains("\\frac") { 3 } else { 0 }
            + if s.contains("\\alpha") { 2 } else { 0 }
            + if s.contains("\\\\") { 2 } else { 0 }
            + (s.matches('\\').count() as i32);

        let typst_score: i32 = if s.contains("#set") { 10 } else { 0 }
            + if s.contains("#show") { 10 } else { 0 }
            + if s.contains("#import") { 8 } else { 0 }
            + if s.starts_with('=') { 5 } else { 0 }
            + if s.contains("\n= ") { 5 } else { 0 }
            + if s.contains("frac(") { 3 } else { 0 }
            + if s.contains("sqrt(") { 3 } else { 0 };

        if latex_score > typst_score + 3 {
            Some(Format::Latex)
        } else if typst_score > latex_score + 3 {
            Some(Format::Typst)
        } else if latex_score > 0 {
            Some(Format::Latex)
        } else if typst_score > 0 {
            Some(Format::Typst)
        } else {
            None
        }
    }
    #[cfg(all(feature = "latex", not(feature = "typst")))]
    pub fn detect_fmt(_: &str) -> Option<Self> {
        Some(Self::Latex)
    }

    #[cfg(all(feature = "typst", not(feature = "latex")))]
    pub fn detect_fmt(_: &str) -> Option<Self> {
        Some(Self::Typst)
    }
}

#[derive(clap::Args)]
#[group(multiple = false)]
struct FormatFlags {
    #[cfg(feature = "typst")]
    #[arg(short, long)]
    typst: bool,

    #[cfg(feature = "latex")]
    #[arg(short, long)]
    latex: bool,
}

impl FormatFlags {
    fn selected(&self) -> Option<Format> {
        #[cfg(feature = "typst")]
        if self.typst {
            return Some(Format::Typst);
        }
        #[cfg(feature = "latex")]
        if self.latex {
            return Some(Format::Latex);
        }
        None
    }
}

#[derive(Parser)]
#[command(arg_required_else_help = true)]
struct Args {
    #[command(flatten)]
    fmt: FormatFlags,

    data: String,
}

static FORMAT_FLAGS: LazyLock<String> = LazyLock::new(|| {
    Format::value_variants()
        .iter()
        .filter_map(|f| {
            let name = f.to_possible_value()?.get_name().to_string();
            let short = name.chars().next()?;
            Some(format!("-{short}/--{name}"))
        })
        .collect::<Vec<_>>()
        .join(" or ")
});

fn main() {
    let args = Args::parse();
    let Some(fmt) = args
        .fmt
        .selected()
        .or_else(|| Format::detect_fmt(&args.data))
    else {
        eprintln!(
            "Unable to autodetect format. Try passing {} instead",
            FORMAT_FLAGS.as_str()
        );
        std::process::exit(1);
    };
    if let Err(e) = match fmt {
        #[cfg(feature = "latex")]
        Format::Latex => render_latex(&args.data),
        #[cfg(feature = "typst")]
        Format::Typst => render_typst(&args.data),
    } {
        eprintln!("{}", e)
    };
}
