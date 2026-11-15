use std::path::Path;

use clap::ValueEnum;

use crate::utils::{ImgFmt, value_names};

#[derive(ValueEnum, Clone, Copy)]
pub enum Fmt {
    #[cfg(feature = "latex")]
    Latex,
    #[cfg(feature = "typst")]
    Typst,
}

impl Fmt {
    // Function implementation taken from tylax
    #[cfg(all(feature = "latex", feature = "typst"))]
    pub fn detect_content(s: &str) -> Option<Self> {
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
            Some(Self::Latex)
        } else if typst_score > latex_score + 3 {
            Some(Self::Typst)
        } else if latex_score > 0 {
            Some(Self::Latex)
        } else if typst_score > 0 {
            Some(Self::Typst)
        } else {
            None
        }
    }

    #[cfg(all(feature = "latex", not(feature = "typst")))]
    pub fn detect_content(_: &str) -> Option<Self> {
        Some(Self::Latex)
    }

    #[cfg(all(feature = "typst", not(feature = "latex")))]
    pub fn detect_content(_: &str) -> Option<Self> {
        Some(Self::Typst)
    }

    pub fn from_extension(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            #[cfg(feature = "latex")]
            "tex" | "latex" => Some(Self::Latex),
            #[cfg(feature = "typst")]
            "typ" | "typst" => Some(Self::Typst),
            _ => None,
        }
    }

    // Explicit flag -> content autodetection -> file extension -> error
    pub fn resolve(forced: Option<Self>, src: &str, path: Option<&Path>) -> Result<Self, String> {
        forced
            .or_else(|| Self::detect_content(src))
            .or_else(|| path.and_then(Self::from_extension))
            .ok_or_else(|| {
                format!(
                    "Unable to detect format. Try passing {} instead",
                    flag_hint()
                )
            })
    }

    // The name clap uses for this variant on the command line.
    pub fn name(self) -> String {
        crate::utils::value_name(&self)
    }
}

fn flag_hint() -> String {
    value_names::<Fmt>()
        .filter_map(|name| {
            let short = name.chars().next()?;
            Some(format!("-{short}/--{name}"))
        })
        .collect::<Vec<_>>()
        .join(" or ")
}

pub fn render(fmt: &Fmt, src: &str, img: &ImgFmt) -> Result<Vec<u8>, String> {
    match fmt {
        #[cfg(feature = "latex")]
        Fmt::Latex => crate::latex::render_latex(src, img),
        #[cfg(feature = "typst")]
        Fmt::Typst => crate::typst::render_typst(src, img),
    }
}
