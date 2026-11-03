//! Spatially localized change survives whole-image averaging and thumbnail aliasing.
#[derive(Default)]
pub struct Visual {
    previous: Vec<u8>,
    w: u32,
    h: u32,
    tiles: Vec<u32>,
    pub details: Difference,
}
#[derive(Default, Clone, serde::Serialize)]
pub struct Difference {
    pub changed_pixels: u64,
    pub changed_fraction: f64,
    pub mean_absolute_error: f64,
    pub changed_bounds: Option<[u32; 4]>,
}
impl Visual {
    pub fn current_rgb(&self) -> &[u8] {
        &self.previous
    }
    pub fn push(&mut self, rgb: &[u8], w: u32, h: u32) -> f64 {
        self.details = Difference::default();
        if self.previous.len() != rgb.len() || self.w != w || self.h != h {
            self.previous.clear();
            self.previous.extend_from_slice(rgb);
            self.w = w;
            self.h = h;
            return 0.;
        }
        let cols = w.div_ceil(16) as usize;
        let rows = h.div_ceil(16) as usize;
        self.tiles.resize(cols * rows, 0);
        self.tiles.fill(0);
        let mut changed = 0u64;
        let mut sum = 0u64;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0, 0);
        for (i, (pixel, old)) in rgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(self.previous.as_chunks::<3>().0)
            .enumerate()
        {
            let difference = [
                pixel[0].abs_diff(old[0]),
                pixel[1].abs_diff(old[1]),
                pixel[2].abs_diff(old[2]),
            ];
            sum += difference.iter().map(|&v| u64::from(v)).sum::<u64>();
            if difference[0].max(difference[1]).max(difference[2]) > 24 {
                let x = i % w as usize;
                let y = i / w as usize;
                self.tiles[y / 16 * cols + x / 16] += 1;
                min_x = min_x.min(x as u32);
                min_y = min_y.min(y as u32);
                max_x = max_x.max(x as u32);
                max_y = max_y.max(y as u32);
                changed += 1;
            }
        }
        let mut score = changed as f64 / (w as f64 * h as f64);
        for (y, row) in self.tiles.chunks(cols).enumerate() {
            for (x, &count) in row.iter().enumerate() {
                let pixels = (w as usize - x * 16).min(16) * (h as usize - y * 16).min(16);
                score = score.max(count as f64 / pixels as f64);
            }
        }
        self.details = Difference {
            changed_pixels: changed,
            changed_fraction: changed as f64 / (w as f64 * h as f64),
            mean_absolute_error: sum as f64 / (w as f64 * h as f64 * 3.),
            changed_bounds: (changed > 0).then_some([
                min_x,
                min_y,
                max_x.saturating_sub(min_x) + 1,
                max_y.saturating_sub(min_y) + 1,
            ]),
        };
        self.previous.copy_from_slice(rgb);
        score
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_transient_and_return_are_both_visible() {
        let mut v = Visual::default();
        let a = vec![0; 640 * 360 * 3];
        let mut b = a.clone();
        for y in 5..13 {
            for x in 5..13 {
                b[(y * 640 + x) * 3] = 255;
            }
        }
        assert_eq!(v.push(&a, 640, 360), 0.);
        assert!(v.push(&b, 640, 360) > 0.04);
        assert_eq!(v.details.changed_pixels, 64);
        assert_eq!(v.details.changed_bounds, Some([5, 5, 8, 8]));
        assert!((v.details.mean_absolute_error - 64. * 255. / (640. * 360. * 3.)).abs() < 1e-10);
        assert!(v.push(&a, 640, 360) > 0.04);
        assert_eq!(v.push(&a, 640, 360), 0.);
        assert!(v.details.changed_bounds.is_none());
    }
}
