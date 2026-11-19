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

pub(super) fn figures(path: &BezPath) -> impl Iterator<Item = &[PathEl]> {
    let mut rest = path.elements();
    std::iter::from_fn(move || {
        if rest.is_empty() { return None; }
        let length = rest[1..].iter().position(|el| matches!(el, PathEl::MoveTo(_)))
            .map_or(rest.len(), |i| i + 1);
        let (figure, tail) = rest.split_at(length);
        rest = tail;
        Some(figure)
    })
}

fn arc(path: &mut BezPath, center: Point, from: Vec2, angle: f64) {
    let radius = from.hypot();
    if radius > 0.0 {
        kurbo::Arc::new(center, (radius, radius), from.atan2(), angle, 0.0)
            .to_cubic_beziers(0.025, |a, b, end| path.curve_to(a, b, end));
    }
}

fn offset_join(out: &mut BezPath, prev: Point, p: Point, next: Point, distance: f64, pen: &Paint) {
    let incoming = (p - prev).normalize();
    let outgoing = (next - p).normalize();
    let a = incoming.turn_90() * distance;
    let b = outgoing.turn_90() * distance;
    out.line_to(p + a);
    let dot = incoming.dot(outgoing);
    let cross = incoming.cross(outgoing);
    if distance == 0.0 || (1.0 + dot) < 1e-10 {
        out.line_to(p + b);
        return;
    }
    let miter = (a + b) / (1.0 + dot);
    if cross * distance >= 0.0 {
        if (miter - a).hypot() <= p.distance(prev) && (miter - b).hypot() <= p.distance(next) {
            out.line_to(p + miter);
        } else {
            out.line_to(p);
        }
    } else {
        let limit = distance.abs() * pen.miter_limit.max(1.0);
        match pen.join {
            0 | 3 if miter.hypot() <= limit => out.line_to(p + miter),
            3 => {
                let axis = miter.normalize();
                for side in [a, b] {
                    let t = (limit - side.dot(axis)) / (miter - side).dot(axis);
                    out.line_to(p + side.lerp(miter, t));
                }
            }
            2 => arc(out, p, a, cross.atan2(dot)),
            _ => {}
        }
    }
    out.line_to(p + b);
}

fn band_side(out: &mut BezPath, points: &[Point], distance: f64, closed: bool, pen: &Paint, start: bool) {
    let n = points.len();
    let first = points[0] + (points[1] - points[0]).normalize().turn_90() * distance;
    if start { out.move_to(first); } else { out.line_to(first); }
    for i in 1..n-1 {
        offset_join(out, points[i-1], points[i], points[i+1], distance, pen);
    }
    if closed {
        offset_join(out, points[n-2], points[n-1], points[0], distance, pen);
        offset_join(out, points[n-1], points[0], points[1], distance, pen);
        out.close_path();
    } else {
        let last = points[n-1] + (points[n-1] - points[n-2]).normalize().turn_90() * distance;
        out.line_to(last);
    }
}

fn band_cap(out: &mut BezPath, end: Point, direction: Vec2, low: f64, high: f64, kind: Cap) {
    let normal = direction.turn_90();
    let radius = (high - low) * 0.5;
    let center = end + normal * ((low + high) * 0.5);
    match kind {
        Cap::Butt => {},
        Cap::Square => {
            out.line_to(end + normal * low + direction * radius);
            out.line_to(end + normal * high + direction * radius);
        }
        Cap::Round => arc(out, center, -normal * radius, std::f64::consts::PI),
    }
    out.line_to(end + normal * high);
}

fn banded_outline(path: &BezPath, style: &Stroke, pen: &Paint) -> BezPath {
    let mut points = Vec::new();
    let mut closed = false;
    kurbo::flatten(path.iter(), 0.025, |el| match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) if points.last() != Some(&p) => points.push(p),
        PathEl::ClosePath => closed = true,
        _ => {}
    });
    if closed && points.first() == points.last() { points.pop(); }
    let mut result = BezPath::new();
    if points.len() < 2 { return result; }
    let compound = if pen.compound.is_empty() { &[0.0, 1.0][..] } else { &pen.compound };
    for pair in compound.chunks_exact(2) {
        let low = (pair[0] - 0.5) * style.width;
        let high = (pair[1] - 0.5) * style.width;
        if low == high { continue; }
        let mut band = BezPath::new();
        band_side(&mut band, &points, low, closed, pen, true);
        let n = points.len();
        if !closed {
            band_cap(&mut band, points[n-1], (points[n-1] - points[n-2]).normalize(), low, high, style.end_cap);
        }
        points.reverse();
        band_side(&mut band, &points, -high, closed, pen, closed);
        if !closed {
            band_cap(&mut band, points[n-1], (points[n-1] - points[n-2]).normalize(), -high, -low, style.start_cap);
            band.close_path();
        }
        points.reverse();
        add_shape(&mut result, band);
    }
    result
}

pub(super) fn outline(path: &BezPath, paint: &Paint) -> Result<BezPath, String> {
    let mut style = Stroke::new(paint.width);
    style.join = match paint.join {
        0 | 3 => Join::Miter,
        1 => Join::Bevel,
        2 => Join::Round,
        _ => return Err("invalid pen join".into()),
    };
    style.miter_limit = paint.miter_limit;
    if paint.alignment == 1 { style.width *= 2.0; }
    let dashes: Vec<_> = paint.dashes.iter().map(|v| v * paint.width).collect();
    let mut result = BezPath::new();
    for figure in figures(path) {
        let figure = BezPath::from_vec(figure.to_vec());
        let ends = endpoints(&figure);
        let dashed = if dashes.is_empty() { figure } else {
            BezPath::from_vec(kurbo::dash(figure.iter(), paint.dash_offset * paint.width, &dashes).collect())
        };
        for piece in figures(&dashed) {
            let piece = BezPath::from_vec(piece.to_vec());
            let mut caps = [paint.start_cap, paint.end_cap];
            if !dashes.is_empty() {
                if !matches!(paint.dash_cap, 0 | 2 | 3) { return Err("invalid dash cap".into()); }
                caps = [paint.dash_cap; 2];
                if let (Some(&(a, _, b, _)), Some(&(p, _, q, _))) = (ends.first(), endpoints(&piece).first()) {
                    if a.distance_squared(p) < 1e-12 { caps[0] = paint.start_cap; }
                    if b.distance_squared(q) < 1e-12 { caps[1] = paint.end_cap; }
                }
            }
            style.start_cap = cap(caps[0])?;
            style.end_cap = cap(caps[1])?;
            let mut outline = if paint.join == 3 || !paint.compound.is_empty() {
                banded_outline(&piece, &style, paint)
            } else {
                kurbo::stroke(piece.iter(), &style, &Default::default(), 0.05)
            };
            add_caps(&mut outline, &piece, caps[0], caps[1], paint)?;
            add_shape(&mut result, outline);
        }
    }
    Ok(result)
}
