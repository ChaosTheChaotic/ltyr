use std::{fmt::Write as FmtWrite, io::Write, sync::LazyLock};

use clap::{Parser, ValueEnum};
use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_parser::parse;
use ratex_render::render_to_png;
use typst::layout::Abs;
use typst_as_lib::{TypstEngine, typst_kit_options::TypstKitFontOptions};
use typst_layout::PagedDocument;
use typst_render::render_merged;

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

fn display_img(data: Vec<u8>) -> std::io::Result<()> {
    let cmd = kitty_graphics_protocol::Command::builder()
        .action(kitty_graphics_protocol::Action::TransmitAndDisplay)
        .format(kitty_graphics_protocol::ImageFormat::Png)
        .quiet(2)
        .build();

    let chunks = cmd.serialize_chunked(&data).unwrap(); // This doesent seem to return the Err variant

    let mut stdout = std::io::stdout().lock();
    for chunk in chunks {
        stdout.write_all(chunk.as_bytes())?;
    }
    stdout.flush()?;
    println!(); // Ensure trailing newline
    Ok(())
}

fn render_latex(latex: &str) -> Result<(), String> {
    let ast = match parse(latex) {
        Ok(a) => a,
        Err(e) => {
            return Err({
                let mut err = format!("LaTeX Parse Err: {}\n", e.message);
                if let Some(pos) = e.loc {
                    let errline = {
                        if pos.start <= pos.end && pos.end <= latex.len() {
                            let padding = " ".repeat(pos.start);
                            let span_len = std::cmp::max(1, pos.end - pos.start);
                            let ptrs = "^".repeat(span_len);
                            format!("{}{}^--- Error here", padding, ptrs)
                        } else {
                            "".to_string()
                        }
                    };
                    let _ = writeln!(err, "{}", latex);
                    let _ = write!(err, "{}", errline);
                }
                err
            });
        }
    };
    let layout = layout(&ast, &LayoutOptions::default());
    let display_list = to_display_list(&layout);
    let imgdata = render_to_png(&display_list, &ratex_render::RenderOptions::default())?;

    display_img(imgdata).map_err(|e| e.to_string())?;
    Ok(())
}

fn render_typst(typst: &str) -> Result<(), String> {
    let typst = format!("#set page(margin: 5pt, width: auto, height: auto)\n{typst}");
    let template = TypstEngine::builder()
        .main_file(typst)
        .search_fonts_with(
            TypstKitFontOptions::default()
                .include_system_fonts(true)
                .include_embedded_fonts(true),
        )
        .build();

    let doc: PagedDocument = template
        .compile()
        .output
        .map_err(|e| format!("Typst compilation failed: {e:?}"))?;

    let imgdata = render_merged(&doc, &typst_render::RenderOptions::default(), Abs::default(), None).encode_png().map_err(|e| e.to_string())?;

    display_img(imgdata).map_err(|e| e.to_string())?;
    Ok(())
}

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
