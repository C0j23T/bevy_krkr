use krkr_shim::layer_ex_draw::Paint;
use vello_gpu::kurbo::{
    self, Affine, BezPath, Cap, Circle, Join, PathEl, Point, Rect, Shape, Stroke, Vec2,
};

fn cap(value: u32) -> Result<Cap, String> {
    match value {
        0 | 3 | 16..=20 | 255 => Ok(Cap::Butt),
        1 => Ok(Cap::Square),
        2 => Ok(Cap::Round),
        _ => Err(format!("unsupported LayerExDraw cap {value}")),
    }
}

fn add_shape(result: &mut BezPath, shape: BezPath) {
    let shape = if result.area() * shape.area() < 0.0 {
        shape.reverse_subpaths()
    } else {
        shape
    };
    result.extend(shape);
}

fn cap_shape(kind: u32, arrow: [f64; 4], width: f64) -> Result<Option<BezPath>, String> {
    let mut shape = BezPath::new();
    match kind {
        0..=2 | 16 => return Ok(None),
        3 => {
            shape.move_to((0., -width / 2.));
            shape.line_to((width / 2., 0.));
            shape.line_to((0., width / 2.));
            shape.close_path();
        }
        17 => {
            return Ok(Some(
                Rect::new(-width / 2., -width / 2., width / 2., width / 2.).to_path(0.01),
            ));
        }
        18 => return Ok(Some(Circle::new((0., 0.), width).to_path(0.01))),
        19 => {
            shape.move_to((-width, 0.));
            shape.line_to((0., -width));
            shape.line_to((width, 0.));
            shape.line_to((0., width));
            shape.close_path();
        }
        20 => {
            shape.move_to((0., 0.));
            shape.line_to((-width * 2., -width));
            shape.line_to((-width * 1.5, 0.));
            shape.line_to((-width * 2., width));
            shape.close_path();
        }
        255 => {
            if arrow[0] <= 0. || arrow[1] <= 0. || ![0., 1.].contains(&arrow[3]) {
                return Err("invalid adjustable arrow cap".into());
            }
            let w = arrow[0] * width;
            let h = arrow[1] * width;
            let inset = arrow[2] * width;
            shape.move_to((-h, -w / 2.));
            shape.line_to((0., 0.));
            shape.line_to((-h, w / 2.));
            if arrow[3] != 0. {
                shape.line_to((-h + inset, 0.));
                shape.close_path();
            } else {
                shape = kurbo::stroke(shape.iter(), &Stroke::new(width), &Default::default(), 0.05);
            }
        }
        _ => return Err("unsupported pen cap".into()),
    }
    Ok(Some(shape))
}

fn endpoints(path: &BezPath) -> Vec<(Point, Vec2, Point, Vec2)> {
    let mut result = Vec::new();
    let mut start = None;
    let mut current = Point::ZERO;
    let mut first = None;
    let mut last = Vec2::ZERO;
    for element in path.iter() {
        match element {
            PathEl::MoveTo(p) => {
                if let (Some(start), Some(first)) = (start, first) {
                    result.push((start, first, current, last));
                }
                start = Some(p);
                current = p;
                first = None;
            }
            PathEl::LineTo(p) => {
                let d = p - current;
                if d.hypot2() > 1e-16 {
                    first.get_or_insert(d);
                    last = d;
                }
                current = p;
            }
            PathEl::QuadTo(c, p) => {
                let d = if (c - current).hypot2() > 1e-16 {
                    c - current
                } else {
                    p - current
                };
                first.get_or_insert(d);
                last = if (p - c).hypot2() > 1e-16 { p - c } else { d };
                current = p;
            }
            PathEl::CurveTo(a, b, p) => {
                let d = if (a - current).hypot2() > 1e-16 {
                    a - current
                } else if (b - current).hypot2() > 1e-16 {
                    b - current
                } else {
                    p - current
                };
                first.get_or_insert(d);
                last = if (p - b).hypot2() > 1e-16 {
                    p - b
                } else if (p - a).hypot2() > 1e-16 {
                    p - a
                } else {
                    d
                };
                current = p;
            }
            PathEl::ClosePath => {
                start = None;
                first = None;
            }
        }
    }
    if let (Some(start), Some(first)) = (start, first) {
        result.push((start, first, current, last));
    }
    result
}

fn add_caps(
    result: &mut BezPath,
    path: &BezPath,
    start: u32,
    end: u32,
    paint: &Paint,
) -> Result<(), String> {
    for (a, ad, b, bd) in endpoints(path) {
        for (point, direction, kind, arrow) in [
            (a, -ad, start, paint.start_arrow),
            (b, bd, end, paint.end_arrow),
        ] {
            if direction.hypot2() <= 1e-16 {
                continue;
            }
            if let Some(shape) = cap_shape(kind, arrow, paint.width)? {
                let d = direction.normalize();
                let t = Affine::new([d.x, d.y, -d.y, d.x, point.x, point.y]);
                add_shape(result, t * shape);
            }
        }
    }
    Ok(())
}

pub(super) fn outline(path: &BezPath, paint: &Paint) -> Result<BezPath, String> {
    let mut stroke = Stroke::new(paint.width);
    stroke.start_cap = cap(paint.start_cap)?;
    stroke.end_cap = cap(paint.end_cap)?;
    stroke.join = match paint.join {
        0 => Join::Miter,
        1 => Join::Bevel,
        2 => Join::Round,
        3 => return Err("miter-clipped join is not supported".into()),
        _ => return Err("invalid pen join".into()),
    };
    stroke.miter_limit = paint.miter_limit;
    stroke.dash_offset = paint.dash_offset * paint.width;
    stroke
        .dash_pattern
        .extend(paint.dashes.iter().map(|v| v * paint.width));
    let path = if !paint.dashes.is_empty() && paint.dash_cap != 0 {
        let dashes = stroke.dash_pattern.clone();
        let dashed =
            BezPath::from_vec(kurbo::dash(path.iter(), stroke.dash_offset, &dashes).collect());
        stroke.dash_pattern.clear();
        stroke.dash_offset = 0.;
        stroke.start_cap = match paint.dash_cap {
            2 => Cap::Round,
            3 => Cap::Butt,
            _ => return Err("invalid dash cap".into()),
        };
        stroke.end_cap = stroke.start_cap;
        dashed
    } else {
        path.clone()
    };
    if paint.alignment == 1 {
        stroke.width *= 2.;
    }
    let mut result = if paint.compound.is_empty() {
        kurbo::stroke(path.iter(), &stroke, &Default::default(), 0.05)
    } else {
        if paint
            .compound
            .iter()
            .zip(paint.compound.iter().rev())
            .any(|(a, b)| (a + b - 1.).abs() > 1e-8)
        {
            return Err("asymmetric compound pen is not supported".into());
        }
        let mut result = BezPath::new();
        for band in paint.compound.chunks_exact(2) {
            if band[1] <= 0.5 {
                continue;
            }
            let outer = (band[1] - 0.5).abs() * 2.;
            let inner = (band[0].max(0.5) - 0.5) * 2.;
            let mut style = stroke.clone();
            style.width = stroke.width * outer;
            result.extend(kurbo::stroke(
                path.iter(),
                &style,
                &Default::default(),
                0.05,
            ));
            if inner > 0. {
                style.width = stroke.width * inner;
                result.extend(
                    kurbo::stroke(path.iter(), &style, &Default::default(), 0.05)
                        .reverse_subpaths(),
                );
            }
        }
        result
    };
    if !paint.dashes.is_empty() && paint.dash_cap == 3 {
        add_caps(&mut result, &path, 3, 3, paint)?;
    }
    add_caps(&mut result, &path, paint.start_cap, paint.end_cap, paint)?;
    Ok(result)
}
