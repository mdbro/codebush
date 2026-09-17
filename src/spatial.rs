use crate::geometry::Rect;

/// Immutable bounding-volume tree. Queries retain the original paint order.
pub struct SpatialIndex {
    bounds: Vec<Rect>,
    order: Vec<usize>,
    nodes: Vec<Node>,
}

struct Node {
    bounds: Rect,
    start: usize,
    end: usize,
    children: Option<[usize; 2]>,
}

impl SpatialIndex {
    pub fn new(bounds: impl IntoIterator<Item = Rect>) -> Self {
        let bounds: Vec<_> = bounds.into_iter().collect();
        let mut index = Self {
            order: (0..bounds.len()).collect(),
            bounds,
            nodes: Vec::new(),
        };
        if !index.order.is_empty() {
            index.build(0, index.order.len());
        }
        index
    }

    fn build(&mut self, start: usize, end: usize) -> usize {
        let first = self.bounds[self.order[start]];
        let (mut x, mut y, mut ex, mut ey) =
            (first.x, first.y, first.x + first.w, first.y + first.h);
        for &id in &self.order[start + 1..end] {
            let b = self.bounds[id];
            x = x.min(b.x);
            y = y.min(b.y);
            ex = ex.max(b.x + b.w);
            ey = ey.max(b.y + b.h);
        }
        let bounds = Rect::new(x, y, ex - x, ey - y);
        let id = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            start,
            end,
            children: None,
        });
        if end - start > 16 {
            let horizontal = bounds.w >= bounds.h;
            let axis = |b: Rect| {
                if horizontal {
                    b.x + b.w * 0.5
                } else {
                    b.y + b.h * 0.5
                }
            };
            let middle = start + (end - start) / 2;
            self.order[start..end].select_nth_unstable_by(middle - start, |&a, &b| {
                axis(self.bounds[a])
                    .total_cmp(&axis(self.bounds[b]))
                    .then(a.cmp(&b))
            });
            let left = self.build(start, middle);
            let right = self.build(middle, end);
            self.nodes[id].children = Some([left, right]);
        }
        id
    }

    pub fn query(&self, view: Rect) -> Vec<usize> {
        // Broad overviews benefit from one sequential scan and need no sorting.
        if self
            .nodes
            .first()
            .is_some_and(|root| view.w * view.h >= root.bounds.w * root.bounds.h * 0.2)
        {
            return self
                .bounds
                .iter()
                .enumerate()
                .filter(|(_, b)| b.intersects(view))
                .map(|(id, _)| id)
                .collect();
        }
        let mut found = Vec::new();
        if !self.nodes.is_empty() {
            self.visit(0, view, &mut found);
        }
        found.sort_unstable();
        found
    }

    fn visit(&self, id: usize, view: Rect, found: &mut Vec<usize>) {
        let node = &self.nodes[id];
        if !node.bounds.intersects(view) {
            return;
        }
        if let Some([left, right]) = node.children {
            self.visit(left, view, found);
            self.visit(right, view, found);
        } else {
            found.extend(
                self.order[node.start..node.end]
                    .iter()
                    .copied()
                    .filter(|&id| self.bounds[id].intersects(view)),
            );
        }
    }
}
