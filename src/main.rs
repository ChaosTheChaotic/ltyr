#[cfg(not(any(feature = "latex", feature = "typst")))]
compile_error!("Enable at least one of latex or typst features");

mod consts;
mod fmt;
#[cfg(feature = "latex")]
mod latex;
mod term;
#[cfg(feature = "typst")]
mod typst;
mod utils;
#[cfg(feature = "watch")]
mod watch;
#[cfg(feature = "watch")]
mod worker;

#[cfg(any(feature = "output", feature = "watch"))]
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{
    Parser, ValueEnum,
    builder::{
        Styles,
        styling::{AnsiColor, Effects},
    },
};

#[cfg(any(feature = "output", feature = "watch"))]
use faccess::PathExt;

use crate::{
    fmt::{Fmt, render},
    utils::{ImgFmt, value_names},
};

#[derive(clap::Args)]
#[group(multiple = false)]
struct FmtFlags {
    #[cfg(feature = "typst")]
    #[arg(short, long)]
    typst: bool,

    #[cfg(feature = "latex")]
    #[arg(short, long)]
    latex: bool,
}

impl FmtFlags {
    fn selected(&self) -> Option<Fmt> {
        #[cfg(feature = "typst")]
        if self.typst {
            return Some(Fmt::Typst);
        }
        #[cfg(feature = "latex")]
        if self.latex {
            return Some(Fmt::Latex);
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
    fmt: FmtFlags,

    #[cfg(feature = "output")]
    #[arg(short, long, value_name = "PATH", value_parser = validate_out, help = format!("Write to PATH instead of displaying. Fmt is taken from the extension ({})", out_fmts()))]
    out: Option<Out>,

    // Re-render file whenever it changes (q, esc or ctrl+c to quit)
    #[cfg(feature = "watch")]
    #[arg(short, long, value_name = "FILE", value_parser = validate_watch, conflicts_with = "data")]
    watch: Option<PathBuf>,

    // Internal: render stdin and write the result to stdout
    // Used by --watch so a render can be killed when the file changes again.
    #[cfg(feature = "watch")]
    #[arg(long = worker::CHILD_FLAG, value_name = "FORMAT", hide = true, conflicts_with_all = ["data", "watch"])]
    render_child: Option<Fmt>,

    #[cfg(feature = "watch")]
    #[arg(long = worker::CHILD_IMG_FLAG, value_name = "IMG", hide = true, requires = "render_child", conflicts_with_all = ["data", "watch"])]
    render_child_img: Option<ImgFmt>,

    #[cfg_attr(
        feature = "watch",
        arg(required_unless_present_any = ["watch", "render_child"])
    )]
    #[cfg_attr(not(feature = "watch"), arg(required = true))]
    data: Option<String>,
}

#[cfg(feature = "output")]
#[derive(Clone)]
struct Out {
    path: PathBuf,
    fmt: ImgFmt,
}

#[cfg(feature = "watch")]
fn validate_watch(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);

    if !path.is_file() {
        return Err(format!("{} is not an existing file", path.display()));
    }

    if !path.readable() {
        return Err(format!("Cannot read file {}", path.display()));
    }

    Ok(path)
}

#[cfg(feature = "output")]
#[inline]
fn out_fmts() -> String {
    value_names::<ImgFmt>().collect::<Vec<_>>().join(", ")
}

#[cfg(feature = "output")]
fn validate_out(s: &str) -> Result<Out, String> {
    let mut path = PathBuf::from(s);
    if path.is_dir() || s.ends_with(std::path::is_separator) {
        path = path.join("out.png");
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
        .and_then(|e| ImgFmt::from_str(e, true).ok())
        .unwrap_or_else(|| {
            warn!(
                "Unrecognised extension, writing png. Supported: {}",
                out_fmts()
            );
            ImgFmt::Png
        });
    Ok(Out { path, fmt })
}

fn main() -> ExitCode {
    let mut args = Args::parse();

    #[cfg(feature = "watch")]
    if let Some(fmt) = args.render_child {
        let img = args.render_child_img.unwrap_or(ImgFmt::Png);
        return worker::run_child(&fmt, &img);
    }

    #[cfg(feature = "watch")]
    if let Some(path) = args.watch.take() {
        #[cfg(feature = "output")]
        let result = watch::run(path, args.fmt.selected(), args.out.take());
        #[cfg(not(feature = "output"))]
        let result = watch::run(path, args.fmt.selected());
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                error!("{e}");
                ExitCode::FAILURE
            }
        };
    }

    render_once(&args)
}

fn render_once(args: &Args) -> ExitCode {
    let Some(data) = args.data.as_deref() else {
        return ExitCode::FAILURE;
    };

    let fmt = match Fmt::resolve(args.fmt.selected(), data, None) {
        Ok(f) => f,
        Err(e) => {
            error!("{e}");
            return ExitCode::FAILURE;
        }
    };

    // One-shot mode keeps the refuse to overwrite
    // Check it before rendering
    #[cfg(feature = "output")]
    if let Some(out) = &args.out
        && out.path.exists()
    {
        error!("The file {} already exists", out.path.display());
        return ExitCode::FAILURE;
    }

    #[cfg(feature = "output")]
    let img_fmt = args.out.as_ref().map_or(ImgFmt::Png, |o| o.fmt);
    #[cfg(not(feature = "output"))]
    let img_fmt = ImgFmt::Png;

    let img = match render(&fmt, data, &img_fmt) {
        Ok(img) => img,
        Err(e) => {
            error!("{e}");
            return ExitCode::FAILURE;
        }
    };

    #[cfg(feature = "output")]
    if let Some(out) = args.out.as_ref() {
        return match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out.path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, &img))
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                error!("{e}");
                ExitCode::FAILURE
            }
        };
    }

    match term::display_img(&img) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}
