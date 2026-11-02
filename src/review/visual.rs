//! Spatially localized change survives whole-image averaging and thumbnail aliasing.
#[derive(Default)]
pub struct Visual {
    previous: Vec<u8>,
    w: u32,
    h: u32,
}
impl Visual {
    pub fn push(&mut self, rgb: &[u8], w: u32, h: u32) -> f64 {
        if self.previous.len() != rgb.len() || self.w != w || self.h != h {
            self.previous.clear();
            self.previous.extend_from_slice(rgb);
            self.w = w;
            self.h = h;
            return 0.;
        }
        let cols = w.div_ceil(16) as usize;
        let rows = h.div_ceil(16) as usize;
        let mut tiles = vec![0u32; cols * rows];
        let mut changed = 0u64;
        for (i, (pixel, old)) in rgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(self.previous.as_chunks::<3>().0)
            .enumerate()
        {
            if pixel[0]
                .abs_diff(old[0])
                .max(pixel[1].abs_diff(old[1]))
                .max(pixel[2].abs_diff(old[2]))
                > 24
            {
                let x = i % w as usize;
                let y = i / w as usize;
                tiles[y / 16 * cols + x / 16] += 1;
                changed += 1;
            }
        }
        let mut score = changed as f64 / (w as f64 * h as f64);
        for (y, row) in tiles.chunks(cols).enumerate() {
            for (x, &count) in row.iter().enumerate() {
                let pixels = (w as usize - x * 16).min(16) * (h as usize - y * 16).min(16);
                score = score.max(count as f64 / pixels as f64);
            }
        }
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
        assert!(v.push(&a, 640, 360) > 0.04);
        assert_eq!(v.push(&a, 640, 360), 0.);
    }
}
