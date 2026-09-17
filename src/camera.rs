use crate::geometry::Rect;
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub center: [f32; 2],
    pub zoom: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: [0., 0.],
            zoom: 1.,
        }
    }
}
impl Camera {
    pub fn fit(&mut self, bounds: Rect, viewport: Rect) {
        self.center = bounds.center();
        self.zoom = ((viewport.w - 64.).max(1.) / bounds.w.max(1.))
            .min((viewport.h - 64.).max(1.) / bounds.h.max(1.))
            .clamp(0.00001, 10000.);
    }
    pub fn world(self, p: [f32; 2], viewport: Rect) -> [f32; 2] {
        let c = viewport.center();
        [
            self.center[0] + (p[0] - c[0]) / self.zoom,
            self.center[1] + (p[1] - c[1]) / self.zoom,
        ]
    }
    pub fn screen(self, p: [f32; 2], viewport: Rect) -> [f32; 2] {
        let c = viewport.center();
        [
            c[0] + (p[0] - self.center[0]) * self.zoom,
            c[1] + (p[1] - self.center[1]) * self.zoom,
        ]
    }
    pub fn view(self, viewport: Rect) -> Rect {
        let p = self.world([viewport.x, viewport.y], viewport);
        Rect::new(p[0], p[1], viewport.w / self.zoom, viewport.h / self.zoom)
    }
    pub fn zoom_at(&mut self, factor: f32, p: [f32; 2], viewport: Rect) {
        let before = self.world(p, viewport);
        self.zoom = (self.zoom * factor).clamp(0.00001, 10000.);
        let after = self.world(p, viewport);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }
    pub fn pan(&mut self, delta: [f32; 2]) {
        self.center[0] -= delta[0] / self.zoom;
        self.center[1] -= delta[1] / self.zoom;
    }
}
