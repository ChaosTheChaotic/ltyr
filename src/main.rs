mod latex;
mod typst;
mod utils;

use std::sync::LazyLock;

use clap::{Parser, ValueEnum};

use crate::latex::render_latex;
use crate::typst::render_typst;

#[derive(clap::ValueEnum, Clone, Copy)]
enum Format {
    Latex,
    Typst,
}

impl Format {
    // Function implementation taken from tylax
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
}

#[derive(clap::Args)]
#[group(multiple = false)]
struct FormatFlags {
    #[arg(short, long)]
    typst: bool,

    #[arg(short, long)]
    latex: bool,
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
    let fmt = if args.fmt.typst {
        Format::Typst
    } else if args.fmt.latex {
        Format::Latex
    } else {
        match Format::detect_fmt(&args.data) {
            Some(f) => f,
            None => {
                eprintln!(
                    "Unable to autodetect format. Try passing {} instead",
                    FORMAT_FLAGS.as_str()
                );
                std::process::exit(1);
            }
        }
    };
    if let Err(e) = match fmt {
        Format::Latex => render_latex(&args.data),
        Format::Typst => render_typst(&args.data),
    } {
        eprintln!("{}", e)
    };
}
