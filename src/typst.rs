use typst::{layout::Abs, utils::Scalar};
use typst_as_lib::{TypstEngine, typst_kit_options::TypstKitFontOptions};
use typst_layout::PagedDocument;
use typst_render::render_merged;
#[cfg(feature = "svg-typst")]
use typst_svg::SvgOptions;

use crate::{
    consts::{EM_PX, PADDING_PX},
    utils::ImgFmt,
};

const TYPST_TXT_PT: f32 = 12.0;

pub fn render_typst(typst: &str, fmt: &ImgFmt) -> Result<Vec<u8>, String> {
    let px_per_pt = EM_PX / TYPST_TXT_PT;
    let margin_pt = PADDING_PX / px_per_pt;
    let typst = format!(
        "#set page(margin: {margin_pt}pt, width: auto, height: auto)\n\
         #set text(size: {TYPST_TXT_PT}pt, top-edge: \"bounds\", bottom-edge: \"bounds\")\n{typst}"
    );

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

    match fmt {
        ImgFmt::Png => render_merged(
            &doc,
            &typst_render::RenderOptions {
                pixel_per_pt: Scalar::new(px_per_pt as f64),
                ..Default::default()
            },
            Abs::default(),
            None,
        )
        .encode_png()
        .map_err(|e| e.to_string()),
        #[cfg(feature = "svg-typst")]
        ImgFmt::Svg => {
            Ok(typst_svg::svg_merged(&doc, &SvgOptions::default(), Abs::default()).into_bytes())
        }
        #[cfg(all(feature = "svg-latex", not(feature = "svg-typst")))]
        _ => Err("This build has no Typst SVG support (enable the `svg-typst` feature)".into()),
    }
}
