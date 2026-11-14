#[cfg(not(any(feature = "latex", feature = "typst")))]
compile_error!("Enable at least one of latex or typst features");

mod consts;
#[cfg(feature = "latex")]
mod latex;
#[cfg(feature = "typst")]
mod typst;
mod utils;

use std::{path::PathBuf, sync::LazyLock};

use clap::{
    Parser, ValueEnum,
    builder::{
        Styles,
        styling::{AnsiColor, Effects},
    },
};

#[cfg(feature = "output")]
use faccess::PathExt;

#[cfg(feature = "latex")]
use crate::latex::render_latex;
#[cfg(feature = "typst")]
use crate::typst::render_typst;

use crate::utils::{ImgFmt, display_img, value_names};

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

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD)) // flag names, possible values
    .placeholder(AnsiColor::Cyan.on_default()) // <DATA>, <PATH>
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default());

#[derive(Parser)]
#[command(arg_required_else_help = true, styles = STYLES)]
struct Args {
    #[command(flatten)]
    fmt: FormatFlags,

    #[cfg(feature = "output")]
    #[arg(short, long, value_name = "PATH", value_parser = validate_out, help = format!("Write to PATH instead of displaying. Format is taken from the extension ({})", out_formats()))]
    out: Option<Out>,

    data: String,
}

#[cfg(feature = "output")]
#[derive(Clone)]
struct Out {
    path: PathBuf,
    fmt: ImgFmt,
}

#[cfg(feature = "output")]
#[inline]
fn out_formats() -> String {
    value_names::<ImgFmt>().collect::<Vec<_>>().join(", ")
}

#[cfg(feature = "output")]
fn validate_out(s: &str) -> Result<Out, String> {
    let mut path = PathBuf::from(s);

    if path.is_dir() || s.ends_with(std::path::is_separator) {
        path = path.join("out.png");
    }

    if path.exists() {
        return Err(format!("The file {} already exists", path.display()));
    }

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));

    if !parent.exists() {
        return Err(format!(
            "The parent folder of {} does not exist. Refusing to write",
            path.display()
        ));
    }
    if !parent.writable() {
        return Err(format!(
            "The parent folder of {} is not writable",
            path.display()
        ));
    }

    let fmt = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(|e| ImgFmt::from_str(e, true).ok()) // case insensitive
        .unwrap_or_else(|| {
            warn!(
                "Unrecognised extension, writing png. Supported: {}",
                out_formats()
            );
            ImgFmt::Png
        });
    Ok(Out { path, fmt })
}

static FORMAT_FLAGS: LazyLock<String> = LazyLock::new(|| {
    value_names::<Format>()
        .filter_map(|name| {
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
        error!(
            "Unable to autodetect format. Try passing {} instead",
            FORMAT_FLAGS.as_str()
        );
        std::process::exit(1);
    };

    #[cfg(feature = "output")]
    let img_fmt = args.out.as_ref().map_or(ImgFmt::Png, |o| o.fmt);
    #[cfg(not(feature = "output"))]
    let img_fmt = ImgFmt::Png;

    let img = match fmt {
        #[cfg(feature = "latex")]
        Format::Latex => render_latex(&args.data, &img_fmt),
        #[cfg(feature = "typst")]
        Format::Typst => render_typst(&args.data, &img_fmt),
    }
    .unwrap_or_else(|e| {
        error!("{}", e);
        std::process::exit(1);
    });

    // We just use a match block here so we can gate the image writing behind the feature
    match () {
        #[cfg(feature = "output")]
        () if args.out.is_some() => {
            std::fs::write(args.out.unwrap().path, &img).unwrap_or_else(|e| error!("{e}"));
        }
        _ => {
            display_img(&img).unwrap_or_else(|e| error!("{e}"));
        }
    }
}
