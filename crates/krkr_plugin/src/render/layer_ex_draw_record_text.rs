use super::{MAX_POINTS, Object, Parser, Reader, region::elements, unit_pixels};
use ttf_parser::{Face,GlyphId,OutlineBuilder};
use vello_gpu::kurbo::{Affine,BezPath,Point};

impl Reader<'_> {
    fn utf16(&mut self, count:usize) -> Result<String,String> {
        let bytes=self.take(count*2)?;
        Ok(char::decode_utf16(bytes.chunks_exact(2).map(|b|u16::from_le_bytes([b[0],b[1]])))
            .map(|r|r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect())
    }
}

#[derive(Clone)]
pub(super) struct Font {
    family:String,
    size:f64,
    unit:u32,
    style:u32,
}

impl Font {
    pub fn read(r:&mut Reader<'_>) -> Result<Self,String> {
        r.version()?;
        let size=r.f32()?;
        let unit=r.u32()?;
        let style=r.u32()?;
        r.take(4)?;
        let len=r.count(32768)?;
        let family=r.utf16(len)?.trim_end_matches('\0').to_owned();
        if size<=0. || unit>6 || style & !15!=0 {return Err("invalid EMF+ font".into());}
        Ok(Self {family,size,unit,style})
    }
}

struct GlyphOutline<'a> {
    path:&'a mut BezPath,
    matrix:Affine,
}

impl GlyphOutline<'_> {
    fn point(&self,x:f32,y:f32)->Point {self.matrix*Point::new(x.into(),y.into())}
}

impl OutlineBuilder for GlyphOutline<'_> {
    fn move_to(&mut self,x:f32,y:f32) {self.path.move_to(self.point(x,y));}
    fn line_to(&mut self,x:f32,y:f32) {self.path.line_to(self.point(x,y));}
    fn quad_to(&mut self,a:f32,b:f32,x:f32,y:f32) {self.path.quad_to(self.point(a,b),self.point(x,y));}
    fn curve_to(&mut self,a:f32,b:f32,c:f32,d:f32,x:f32,y:f32) {self.path.curve_to(self.point(a,b),self.point(c,d),self.point(x,y));}
    fn close(&mut self) {self.path.close_path();}
}

fn glyph(path:&mut BezPath,face:&Face<'_>,id:GlyphId,size:f64,x:f64,y:f64) -> Result<(),String> {
    let scale=size/f64::from(face.units_per_em());
    let matrix=Affine::new([scale,0.,0.,-scale,x,y]);
    face.outline_glyph(id,&mut GlyphOutline {path,matrix});
    if path.elements().len()>MAX_POINTS {return Err("EMF+ glyph outline limit".into());}
    Ok(())
}

impl Parser<'_> {
    pub(super) fn driver_string(&mut self,flags:u16,r:&mut Reader<'_>) -> Result<(),String> {
        let paint=self.brush(flags,r.u32()?)?;
        let Some(Object::Font(font))=self.objects.get(&(flags as u8)) else {return Err("missing EMF+ font".into());};
        let size=if font.unit==0 {font.size} else {
            font.size*unit_pixels(font.unit,self.logical_dpi)?[1]/unit_pixels(self.state.page_unit,self.logical_dpi)?[1]
        };
        let path={
            let options=r.u32()?;
            let matrix_present=r.u32()?;
            let count=r.count(1<<20)?;
            if options&!15!=0 || matrix_present>1 {return Err("invalid EMF+ driver string".into());}
            let codes=(0..count).map(|_|r.u16()).collect::<Result<Vec<_>,_>>()?;
            let realized=options&4!=0;
            let positions=(0..if realized {count.min(1)} else {count}).map(|_|r.floats::<2>()).collect::<Result<Vec<_>,_>>()?;
            let matrix=if matrix_present!=0 {Affine::new(r.floats()?)} else {Affine::IDENTITY};
            let path=krkr_shim::font::with_default_font(&font.family,font.style,|bytes,index| {
                let face=Face::parse(bytes,index).map_err(|_|"invalid EMF+ driver font")?;
                let mut path=BezPath::new();
                let mut pos=positions.first().copied().unwrap_or_default();
                for (i,c) in codes.into_iter().enumerate() {
                    if !realized {pos=positions[i];}
                    let id=if options&1==0 {GlyphId(c)} else {char::from_u32(c.into()).and_then(|ch|face.glyph_index(ch)).unwrap_or(GlyphId(0))};
                    glyph(&mut path,&face,id,size,pos[0],pos[1])?;
                    let vertical=options&2!=0;
                    let advance=if vertical {face.glyph_ver_advance(id)} else {face.glyph_hor_advance(id)};
                    pos[usize::from(vertical)]+=size*f64::from(advance.unwrap_or(face.units_per_em()))/f64::from(face.units_per_em());
                }
                Ok::<_,String>(matrix*path)
            }).ok_or("EMF+ driver font is unavailable")??;
            path
        };
        self.emit(elements(&path),1,paint)
    }
}
