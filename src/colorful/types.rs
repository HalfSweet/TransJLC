use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use gerber_parser::{gerber_types::*, parse};
use image::ImageReader;
use std::fs;
use std::io::{BufReader, Cursor};
use std::path::Path;

/// Geometry extracted from a solder mask layer, represented as SVG path data strings
/// in 10-mil coordinates with inverted Y (matching the SVG output).
pub(crate) type MaskPaths = Vec<String>;

/// Board outline bounds expressed in millimeters.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BoardBounds {
    pub(crate) min_x: f64,
    pub(crate) max_x: f64,
    pub(crate) min_y: f64,
    pub(crate) max_y: f64,
}

impl BoardBounds {
    pub(crate) fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub(crate) fn height(&self) -> f64 {
        self.max_y - self.min_y
    }

    pub(crate) fn origin(&self) -> (f64, f64) {
        (self.min_x, self.min_y)
    }
}

/// Loaded image metadata and base64 data URI.
pub(crate) struct SilkscreenImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) data_uri: String,
}

pub(crate) fn parse_outline_bounds(content: &str) -> Result<BoardBounds> {
    let reader = BufReader::new(Cursor::new(content));
    let doc = match parse(reader) {
        Ok(doc) => doc,
        Err((partial, err)) => {
            if partial.commands().is_empty() {
                return Err(anyhow!("Failed to parse outline: {err}"));
            }
            partial
        }
    };

    let mut units = doc.units.unwrap_or(Unit::Millimeters);

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for cmd in doc.commands() {
        if let Command::FunctionCode(FunctionCode::DCode(DCode::Operation(op))) = cmd {
            let coords_opt = match op {
                Operation::Interpolate(c, _) => c.as_ref(),
                Operation::Move(c) => c.as_ref(),
                Operation::Flash(c) => c.as_ref(),
            };

            if let Some(coords) = coords_opt {
                if let Some(x) = coords.x {
                    let mut x_val: f64 = x.into();
                    if matches!(units, Unit::Inches) {
                        x_val *= 25.4;
                    }
                    min_x = min_x.min(x_val);
                    max_x = max_x.max(x_val);
                }

                if let Some(y) = coords.y {
                    let mut y_val: f64 = y.into();
                    if matches!(units, Unit::Inches) {
                        y_val *= 25.4;
                    }
                    min_y = min_y.min(y_val);
                    max_y = max_y.max(y_val);
                }
            }
        } else if let Command::ExtendedCode(ExtendedCode::Unit(u)) = cmd {
            units = *u;
        }
    }

    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        bail!("Failed to parse board outline bounds");
    }

    Ok(BoardBounds {
        min_x,
        max_x,
        min_y,
        max_y,
    })
}

pub(crate) fn load_image(path: &Path) -> Result<SilkscreenImage> {
    let bytes = fs::read(path).with_context(|| format!("Read image {}", path.display()))?;
    let reader = ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .context("Guess image format")?;
    let (width, height) = reader.into_dimensions().context("Read image dimensions")?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "png".to_string());
    let encoded = general_purpose::STANDARD.encode(bytes);
    let data_uri = format!("data:image/{};base64,{}", ext, encoded);

    Ok(SilkscreenImage {
        width,
        height,
        data_uri,
    })
}

pub(crate) fn mm_to_mil_10(val: f64) -> f64 {
    val / 0.254
}

pub(crate) fn compute_mark_points(bounds: &BoardBounds) -> Vec<(f64, f64)> {
    // Inset by approximately 3 mil (0.0762mm) to mirror the original script behaviour.
    const INSET_MM: f64 = 0.0762;
    let min_x = bounds.min_x + INSET_MM;
    let max_x = bounds.max_x - INSET_MM;
    let min_y = bounds.min_y + INSET_MM;
    let max_y = bounds.max_y - INSET_MM;

    vec![(min_x, min_y), (min_x, max_y), (max_x, max_y)]
}
