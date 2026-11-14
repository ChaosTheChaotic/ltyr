use std::fmt::Write;

use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_parser::parse;
use ratex_render::render_to_png;

use crate::{
    consts::{EM_PX, PADDING_PX},
    utils::ImgFmt,
};

#[cfg(feature = "svg-latex")]
use ratex_svg::{SvgOptions, render_to_svg};

pub fn render_latex(latex: &str, fmt: &ImgFmt) -> Result<Vec<u8>, String> {
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
    match fmt {
        ImgFmt::Png => render_to_png(
            &display_list,
            &ratex_render::RenderOptions {
                font_size: EM_PX,
                padding: PADDING_PX,
                device_pixel_ratio: 1.0,
                ..Default::default()
            },
        ),
        #[cfg(feature = "svg-latex")]
        ImgFmt::Svg => Ok(render_to_svg(
            &display_list,
            &SvgOptions {
                font_size: EM_PX as _, // as _ so it works whether the field is f32 or f64
                padding: PADDING_PX as _,
                embed_glyphs: true, // standalone svg, no webfont dependency
                ..Default::default()
            },
        )
        .into_bytes()),
        #[cfg(all(feature = "svg-typst", not(feature = "svg-latex")))]
        _ => Err("This build has no LaTeX SVG support (enable the `svg-latex` feature)".into()),
    }
}
