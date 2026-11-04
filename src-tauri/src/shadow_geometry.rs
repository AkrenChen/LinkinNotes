#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowLayout { pub width: i32, pub height: i32, pub padding: i32, pub body_width: i32, pub body_height: i32, pub dpi: u32 }
impl ShadowLayout {
    pub fn new(width: i32, height: i32, dpi: u32) -> Self {
        let padding=(28.0 * dpi as f64 / 96.0).ceil() as i32;
        Self { width: width+padding*2, height: height+padding*2, padding, body_width: width, body_height: height, dpi }
    }
    pub fn pixels(self) -> Vec<u8> {
        let mut pixels=vec![0; self.width as usize * self.height as usize * 4];
        let scale=self.dpi as f64/96.0;
        let radius=9.0*scale;
        let half_width=self.body_width as f64/2.0;
        let half_height=self.body_height as f64/2.0;
        let distance=|x: f64,y: f64| {
            let qx=x.abs()-half_width+radius;
            let qy=y.abs()-half_height+radius;
            qx.max(0.0).hypot(qy.max(0.0))+qx.max(qy).min(0.0)-radius
        };
        for y in 0..self.height {
            for x in 0..self.width {
                let cx=x as f64+0.5-self.padding as f64-half_width;
                let cy=y as f64+0.5-self.padding as f64-half_height;
                if (cx.abs()<=half_width-radius && cy.abs()<=half_height) || (cy.abs()<=half_height-radius && cx.abs()<=half_width) || distance(cx,cy)<=0.0 { continue; }
                let soft=distance(cx,cy-4.0*scale).max(0.0)/(8.0*scale);
                let contact=distance(cx,cy-scale).max(0.0)/(2.0*scale);
                let alpha=0.16*(-0.5*soft*soft).exp()+0.08*(-0.5*contact*contact).exp();
                pixels[((y*self.width+x)*4+3) as usize]=(alpha*255.0) as u8;
            }
        }
        pixels
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn alpha(layout: ShadowLayout, pixels: &[u8], x: i32, y: i32) -> u8 { pixels[((y * layout.width + x) * 4 + 3) as usize] }
    #[test]
    fn shadow_extends_outside_without_painting_the_body() {
        let layout=ShadowLayout::new(100,80,96);
        assert_eq!(layout.padding,28);
        assert_eq!((layout.width,layout.height),(156,136));
        let pixels=layout.pixels();
        assert_eq!(alpha(layout,&pixels,layout.padding+50,layout.padding+40),0);
        assert!(alpha(layout,&pixels,layout.padding+50,layout.padding-2)>0);
        assert!(alpha(layout,&pixels,layout.padding+50,layout.padding+81)>0);
        assert_eq!(alpha(layout,&pixels,layout.width/2,0),0);
        assert_eq!(alpha(layout,&pixels,layout.width/2,layout.height-1),0);
        assert!(pixels.chunks_exact(4).all(|pixel|pixel[..3]==[0,0,0]));
    }
    #[test]
    fn dpi_scales_the_shadow_without_changing_body_dimensions() {
        let layout=ShadowLayout::new(200,160,192);
        assert_eq!(layout.padding,56);
        assert_eq!((layout.width,layout.height),(312,272));
        assert_eq!((layout.body_width,layout.body_height),(200,160));
        let pixels=layout.pixels();
        assert_eq!(alpha(layout,&pixels,layout.padding+100,layout.padding+80),0);
        assert!(alpha(layout,&pixels,layout.padding+100,layout.padding-4)>0);
    }
}
