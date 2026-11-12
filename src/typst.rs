use typst::layout::Abs;
use typst_as_lib::{TypstEngine, typst_kit_options::TypstKitFontOptions};
use typst_layout::PagedDocument;
use typst_render::render_merged;

use crate::utils::display_img;

pub fn render_typst(typst: &str) -> Result<(), String> {
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

    let imgdata = render_merged(
        &doc,
        &typst_render::RenderOptions::default(),
        Abs::default(),
        None,
    )
    .encode_png()
    .map_err(|e| e.to_string())?;

    display_img(imgdata).map_err(|e| e.to_string())?;
    Ok(())
}
