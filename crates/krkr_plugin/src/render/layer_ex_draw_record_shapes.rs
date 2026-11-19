use super::{MAX_POINTS, Object, Parser, PathElement, Reader};
use std::f64::consts::TAU;
use vello_gpu::kurbo::{Arc, Point};

fn vertex(verb: u32, [x, y]: [f64; 2]) -> PathElement {
    PathElement {
        verb,
        reserved: 0,
        points: [x, y, 0., 0., 0., 0.],
    }
}

fn cubic(a: Point, b: Point, end: Point) -> PathElement {
    PathElement {
        verb: 2,
        reserved: 0,
        points: [a.x, a.y, b.x, b.y, end.x, end.y],
    }
}

impl Reader<'_> {
    pub(super) fn shape_rect(&mut self, flags: u16) -> Result<[f64; 4], String> {
        if flags & 0x4000 == 0 {
            return self.floats();
        }
        let mut rect = [0.; 4];
        for coord in &mut rect {
            *coord = f64::from(self.u16()? as i16);
        }
        Ok(rect)
    }
}

fn ellipse(rect: [f64; 4], start: f64, sweep: f64, pie: bool, closed: bool) -> Vec<PathElement> {
    let [x, y, w, h] = rect;
    if w <= 0. || h <= 0. || sweep == 0. {
        return vec![];
    }
    let (rx, ry) = (w / 2., h / 2.);
    let (cx, cy) = (x + rx, y + ry);
    let parameter = |angle: f64| (rx * angle.sin()).atan2(ry * angle.cos());
    let a = parameter(start.to_radians());
    let b = parameter((start + sweep.clamp(-360., 360.)).to_radians());
    let delta = if sweep.abs() >= 360. {
        sweep.signum() * TAU
    } else {
        sweep.signum() * ((b - a) * sweep.signum()).rem_euclid(TAU)
    };
    let arc = Arc::new((cx, cy), (rx, ry), a, delta, 0.);
    let origin = [cx + rx * a.cos(), cy + ry * a.sin()];
    let mut path = if pie && sweep.abs() < 360. {
        vec![vertex(0, [cx, cy]), vertex(1, origin)]
    } else {
        vec![vertex(0, origin)]
    };
    arc.to_cubic_beziers(0.1, |a, b, d| {
        path.push(cubic(a, b, d));
    });
    if closed {
        path.push(vertex(3, [0.; 2]));
    }
    path
}

fn spline(
    points: &[[f64; 2]],
    offset: usize,
    segments: usize,
    tension: f64,
    closed: bool,
) -> Result<Vec<PathElement>, String> {
    let n = points.len();
    if n < 2 || offset >= n || segments == 0 || (!closed && segments >= n - offset) {
        return Err("invalid EMF+ spline range".into());
    }
    let at = |i: isize| {
        let [x, y] = points[if closed {
            i.rem_euclid(n as isize)
        } else {
            i.clamp(0, n as isize - 1)
        } as usize];
        Point::new(x, y)
    };
    let mut path = vec![vertex(0, points[offset])];
    for i in offset..offset + segments {
        let i = i as isize;
        let (a, b) = (at(i), at(i + 1));
        path.push(cubic(
            a + (b - at(i - 1)) * (tension / 3.),
            b - (at(i + 2) - a) * (tension / 3.),
            b,
        ));
    }
    if closed {
        path.push(vertex(3, [0.; 2]));
    }
    Ok(path)
}

impl Parser<'_> {
    pub(super) fn shape(
        &mut self,
        kind: u16,
        flags: u16,
        r: &mut Reader<'_>,
    ) -> Result<(), String> {
        let paint = if matches!(kind, 0x400a | 0x400c | 0x400e | 0x4010 | 0x4016) {
            self.brush(flags, r.u32()?)?
        } else if let Some(Object::Pen(p)) = self.objects.get(&(flags as u8)) {
            p.clone()
        } else {
            return Err("missing EMF+ shape pen".into());
        };
        let mut path = Vec::new();
        match kind {
            0x400a | 0x400b => {
                let count = r.count(MAX_POINTS / 5)?;
                for _ in 0..count {
                    let [x, y, w, h] = r.shape_rect(flags)?;
                    let rectangle = vec![
                        vertex(0, [x, y]),
                        vertex(1, [x + w, y]),
                        vertex(1, [x + w, y + h]),
                        vertex(1, [x, y + h]),
                        vertex(3, [0.; 2]),
                    ];
                    // Overlapping rectangles are painted individually, including their alpha.
                    self.emit(rectangle, 0, paint.clone())?;
                }
                return Ok(());
            }
            0x400c | 0x400d | 0x4019 => {
                let n = r.count(MAX_POINTS)?;
                let p = r.points(n, flags)?;
                if n < 2 {
                    return Err("too few EMF+ shape points".into());
                }
                path.push(vertex(0, p[0]));
                if kind == 0x4019 {
                    if n < 4 || (n - 1) % 3 != 0 {
                        return Err("invalid EMF+ Bezier count".into());
                    }
                    for c in p[1..].chunks_exact(3) {
                        path.push(cubic(
                            Point::new(c[0][0], c[0][1]),
                            Point::new(c[1][0], c[1][1]),
                            Point::new(c[2][0], c[2][1]),
                        ));
                    }
                } else {
                    path.extend(p[1..].iter().map(|&p| vertex(1, p)));
                    if kind == 0x400c || flags & 0x2000 != 0 {
                        path.push(vertex(3, [0.; 2]));
                    }
                }
            }
            0x400e..=0x4012 => {
                let (start, sweep) = if kind < 0x4010 {
                    (0., 360.)
                } else {
                    (r.f32()?, r.f32()?)
                };
                path = ellipse(
                    r.shape_rect(flags)?,
                    start,
                    sweep,
                    matches!(kind, 0x4010 | 0x4011),
                    kind != 0x4012,
                );
            }
            0x4016..=0x4018 => {
                let tension = r.f32()?;
                let range = if kind == 0x4018 {
                    Some((r.count(MAX_POINTS)?, r.count(MAX_POINTS)?))
                } else {
                    None
                };
                let n = r.count(MAX_POINTS)?;
                let points = r.points(n, flags)?;
                let (offset, segments) = range.unwrap_or((0, n));
                path = spline(&points, offset, segments, tension, range.is_none())?;
            }
            _ => unreachable!(),
        }
        if !path.is_empty() {
            self.emit(
                path,
                u32::from(kind == 0x4016 && flags & 0x2000 != 0),
                paint,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Record, solid};
    use super::*;

    #[test]
    fn compressed_rects_relative_lines_and_partial_spline() {
        let mut record = Record::new(32, 32, 0).unwrap();
        let mut p = Parser::new(&mut record);
        p.objects.insert(3, Object::Pen(solid(0xffffffff)));
        p.shape(
            0x400a,
            0xc000,
            &mut Reader::new(&[1, 2, 3, 255, 1, 0, 0, 0, 255, 255, 2, 0, 5, 0, 6, 0]),
        )
        .unwrap();
        p.shape(
            0x400d,
            0x2803,
            &mut Reader::new(&[2, 0, 0, 0, 1, 2, 0x7f, 3]),
        )
        .unwrap();
        assert_eq!(p.record.operations[0].draw.path[2].points[..2], [4., 8.]);
        let lines = &p.record.operations[1].draw.path;
        assert_eq!(lines[1].points[..2], [0., 5.]);
        assert_eq!(lines[2].verb, 3);
        let mut bytes = Vec::new();
        bytes.extend(0.5f32.to_le_bytes());
        for n in [1u32, 1, 4] {
            bytes.extend(n.to_le_bytes());
        }
        for n in [0i16, 0, 1, 1, 4, 2, 5, 1] {
            bytes.extend(n.to_le_bytes());
        }
        p.shape(0x4018, 0x4003, &mut Reader::new(&bytes)).unwrap();
        assert_eq!(p.record.operations[2].draw.path[1].points[4..], [4., 2.]);
        bytes[8] = 3;
        assert!(p.shape(0x4018, 0x4003, &mut Reader::new(&bytes)).is_err());
    }
}
