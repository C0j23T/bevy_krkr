use super::{Clip, IDENTITY, MAX_BYTES, MAX_POINTS, PathElement, Reader, path_read, rectangle};
use linesweeper::topology::{BinaryWindingNumber, Topology};
use std::sync::Arc;
use vello_gpu::kurbo::{Affine, BezPath, PathEl, Point};

pub(super) fn elements(path: &BezPath) -> Vec<PathElement> {
    let mut current = Point::ZERO;
    path.iter().map(|el| {
        let (verb, points) = match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => {
                current = p;
                (u32::from(matches!(el, PathEl::LineTo(_))), [p.x,p.y,0.,0.,0.,0.])
            }
            PathEl::QuadTo(q, p) => {
                let a = current.lerp(q, 2./3.);
                let b = p.lerp(q, 2./3.);
                current = p;
                (2, [a.x,a.y,b.x,b.y,p.x,p.y])
            }
            PathEl::CurveTo(a,b,p) => {
                current = p;
                (2, [a.x,a.y,b.x,b.y,p.x,p.y])
            }
            PathEl::ClosePath => (3, [0.;6]),
        };
        PathElement {verb, reserved:0, points}
    }).collect()
}

#[derive(Clone, Debug)]
pub(super) struct Region {
    path: Arc<BezPath>,
    fill: u32,
    inverse: bool,
}

impl Region {
    pub fn infinite() -> Self {
        Self {path:Arc::default(), fill:1, inverse:true}
    }

    pub fn from_path(path: &[PathElement], fill:u32) -> Self {
        let mut closed = BezPath::new();
        let mut open = false;
        for el in super::super::super::layer_ex_draw::bez_path(path).iter() {
            if matches!(el, PathEl::MoveTo(_)) && open { closed.close_path(); }
            closed.push(el);
            open = !matches!(el,PathEl::ClosePath);
        }
        if open {closed.close_path();}
        Self {path:Arc::new(closed),fill,inverse:false}
    }

    pub fn transform(&self, matrix:[f64;6]) -> Self {
        Self {path:Arc::new(Affine::new(matrix)*self.path.as_ref()), ..self.clone()}
    }

    pub fn combine(&self, other:&Self, mode:u16) -> Result<Self,String> {
        if mode==0 {return Ok(other.clone());}
        if mode>5 {return Err("invalid EMF+ region combination".into());}
        let combine = |a,b| match mode {1=>a && b,2=>a || b,3=>a ^ b,4=>a && !b,5=>b && !a,_=>unreachable!()};
        let inverse = combine(self.inverse, other.inverse);
        if self.path.is_empty() || other.path.is_empty() {
            let (shape, positive) = if self.path.is_empty() {
                (other,combine(self.inverse, !other.inverse))
            } else {
                (self,combine(!self.inverse, other.inverse))
            };
            return Ok(if positive==inverse {
                Self {path:Arc::default(),fill:1,inverse}
            } else {Self {inverse,..shape.clone()}});
        }
        let inside = |w:i32, r:&Self| (if r.fill==0 {w%2!=0} else {w!=0}) ^ r.inverse;
        let topology=Topology::from_paths_binary(&self.path,&other.path,1e-5).map_err(|e|e.to_string())?;
        let contours=topology.contours(|w:&BinaryWindingNumber| combine(inside(w.shape_a,self),inside(w.shape_b,other)) ^ inverse);
        let mut path=BezPath::new();
        for contour in contours.contours() {
            if path.elements().len()+contour.path.elements().len()>MAX_POINTS {return Err("metafile region point limit".into());}
            path.extend(contour.path.iter());
        }
        Ok(Self {path:Arc::new(path),fill:1,inverse})
    }

    pub fn clip(&self, bounds:[f64;4]) -> Result<Option<Clip>,String> {
        if self.inverse && self.path.is_empty() {return Ok(None);}
        let bounded;
        let region=if self.inverse {
            bounded=self.combine(&Self::from_path(&rectangle(bounds),1),1)?;
            &bounded
        } else {self};
        Ok(Some(Clip {path:elements(&region.path).into(), fill:region.fill, transform:IDENTITY}))
    }

    pub fn read(r:&mut Reader<'_>) -> Result<Self,String> {
        r.version()?;
        let children=r.count(65536)?;
        let mut remaining=children+1;
        let region=Self::node(r,0,&mut remaining)?;
        if remaining!=0 {return Err("invalid EMF+ region child count".into());}
        Ok(region)
    }

    fn node(r:&mut Reader<'_>, depth:usize, remaining:&mut usize) -> Result<Self,String> {
        if depth>=64 || *remaining==0 {return Err("invalid EMF+ region tree".into());}
        *remaining-=1;
        Ok(match r.u32()? {
            mode @ 1..=5 => {
                let a=Self::node(r,depth+1,remaining)?;
                let b=Self::node(r,depth+1,remaining)?;
                a.combine(&b,mode as u16)?
            }
            0x10000000 => Self::from_path(&rectangle(r.floats()?),0),
            0x10000001 => {
                let len=r.count(MAX_BYTES)?;
                let (path,fill)=path_read(r.take(len)?)?;
                Self::from_path(&path,fill)
            }
            0x10000002 => Self {inverse:false,..Self::infinite()},
            0x10000003 => Self::infinite(),
            _ => return Err("invalid EMF+ region node".into()),
        })
    }
}
