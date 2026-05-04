use super::types::{mm_to_mil_10, MaskPaths};
use anyhow::{anyhow, Result};
use gerber_parser::{gerber_types::*, parse};
use std::io::{BufReader, Cursor};

fn coords_to_mm(coords: &Option<Coordinates>, last: (f64, f64), units: Unit) -> (f64, f64) {
    let mut x = last.0;
    let mut y = last.1;
    if let Some(coords) = coords {
        if let Some(cx) = coords.x {
            let mut val: f64 = cx.into();
            if matches!(units, Unit::Inches) {
                val *= 25.4;
            }
            x = val;
        }
        if let Some(cy) = coords.y {
            let mut val: f64 = cy.into();
            if matches!(units, Unit::Inches) {
                val *= 25.4;
            }
            y = val;
        }
    }
    (x, y)
}

fn to_svg_space(x_mm: f64, y_mm: f64) -> (f64, f64) {
    (mm_to_mil_10(x_mm), -mm_to_mil_10(y_mm))
}

fn rect_path(center: (f64, f64), w_mm: f64, h_mm: f64) -> String {
    let (cx, cy) = center;
    let hw = mm_to_mil_10(w_mm) / 2.0;
    let hh = mm_to_mil_10(h_mm) / 2.0;
    let x0 = cx - hw;
    let x1 = cx + hw;
    let y0 = cy - hh;
    let y1 = cy + hh;
    format!("M {x0} {y0} L {x1} {y0} {x1} {y1} {x0} {y1} {x0} {y0} ")
}

fn path_from_region(points: &[(f64, f64)]) -> Option<String> {
    if points.is_empty() {
        return None;
    }
    let mut d = String::new();
    for (idx, (x, y)) in points.iter().enumerate() {
        if idx == 0 {
            d.push_str(&format!("M {} {} ", x, y));
        } else {
            d.push_str(&format!("L {} {} ", x, y));
        }
    }
    d.push('Z');
    Some(d)
}

fn aperture_bbox(ap: &Aperture) -> Option<(f64, f64)> {
    match ap {
        Aperture::Circle(c) => Some((c.diameter, c.diameter)),
        Aperture::Rectangle(r) | Aperture::Obround(r) => Some((r.x, r.y)),
        Aperture::Macro(name, args) if name == "RoundRect" => {
            parse_round_rect_macro(args.as_deref())
        }
        _ => None,
    }
}

fn parse_round_rect_macro(args: Option<&[MacroDecimal]>) -> Option<(f64, f64)> {
    let args = args?;
    if args.len() < 3 {
        return None;
    }
    // Skip radius (first value)
    let coords = &args[1..];
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for pair in coords.chunks(2) {
        if pair.len() == 2 {
            if let (MacroDecimal::Value(x), MacroDecimal::Value(y)) = (&pair[0], &pair[1]) {
                xs.push(x);
                ys.push(y);
            }
        }
    }
    if xs.is_empty() || ys.is_empty() {
        return None;
    }
    let w = xs.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(*b))
        - xs.iter().fold(f64::INFINITY, |a, &b| a.min(*b));
    let h = ys.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(*b))
        - ys.iter().fold(f64::INFINITY, |a, &b| a.min(*b));
    Some((w.abs(), h.abs()))
}

pub fn parse_solder_mask(content: &str) -> Result<MaskPaths> {
    let reader = BufReader::new(Cursor::new(content));
    let doc = match parse(reader) {
        Ok(doc) => doc,
        Err((partial, err)) => {
            if partial.commands().is_empty() {
                return Err(anyhow!("Failed to parse solder mask: {err}"));
            }
            partial
        }
    };

    let mut units = doc.units.unwrap_or(Unit::Millimeters);
    let mut current_aperture: Option<&Aperture> = None;
    let mut current_pos: (f64, f64) = (0.0, 0.0);
    let mut region_active = false;
    let mut region_points: Vec<(f64, f64)> = Vec::new();
    let mut shapes: MaskPaths = Vec::new();
    let mut interp_mode = InterpolationMode::Linear;

    let apertures = &doc.apertures;

    for command in doc.commands() {
        match command {
            Command::ExtendedCode(ExtendedCode::Unit(u)) => units = *u,
            Command::FunctionCode(FunctionCode::GCode(g)) => match g {
                GCode::InterpolationMode(m) => interp_mode = *m,
                GCode::RegionMode(on) => {
                    if !on && region_active && region_points.len() > 1 {
                        if let Some(d) = path_from_region(&region_points) {
                            shapes.push(d);
                        }
                        region_points.clear();
                    }
                    region_active = *on;
                }
                _ => {}
            },
            Command::FunctionCode(FunctionCode::DCode(d)) => match d {
                DCode::SelectAperture(code) => current_aperture = apertures.get(code),
                DCode::Operation(op) => match op {
                    Operation::Move(coords) => {
                        current_pos = coords_to_mm(coords, current_pos, units);
                    }
                    Operation::Interpolate(coords, _) => {
                        let next = coords_to_mm(coords, current_pos, units);
                        if region_active {
                            if region_points.is_empty() {
                                let (sx, sy) = to_svg_space(current_pos.0, current_pos.1);
                                region_points.push((sx, sy));
                            }
                            let (nx, ny) = to_svg_space(next.0, next.1);
                            region_points.push((nx, ny));
                        }
                        current_pos = next;
                        let _ = interp_mode; // currently unused (linearized)
                    }
                    Operation::Flash(coords) => {
                        let pos_mm = coords_to_mm(coords, current_pos, units);
                        let (cx, cy) = to_svg_space(pos_mm.0, pos_mm.1);
                        let ap_ref = current_aperture.or_else(|| {
                            apertures
                                .iter()
                                .max_by_key(|(code, _)| *code)
                                .map(|(_, ap)| ap)
                        });
                        if let Some(ap) = ap_ref {
                            if let Some((w, h)) = aperture_bbox(ap) {
                                shapes.push(rect_path((cx, cy), w, h));
                            }
                        }
                        current_pos = pos_mm;
                    }
                },
            },
            _ => {}
        }
    }

    // Close any pending region
    if region_active && region_points.len() > 1 {
        if let Some(d) = path_from_region(&region_points) {
            shapes.push(d);
        }
    }

    Ok(shapes)
}
