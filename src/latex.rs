use std::fmt::Write;

use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_parser::parse;
use ratex_render::render_to_png;

use crate::{
    consts::{EM_PX, PADDING_PX},
    utils::display_img,
};

pub fn render_latex(latex: &str) -> Result<(), String> {
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
    let imgdata = render_to_png(
        &display_list,
        &ratex_render::RenderOptions {
            font_size: EM_PX,
            padding: PADDING_PX,
            device_pixel_ratio: 1.0,
            ..Default::default()
        },
    )?;

    display_img(&imgdata)?;
    Ok(())
}
