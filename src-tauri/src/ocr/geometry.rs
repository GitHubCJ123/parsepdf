use super::BBoxPx;

#[derive(Debug, Clone, Copy)]
pub struct PageGeometry {
    pub media_box: [f32; 4],
    pub crop_box: [f32; 4],
    pub rotation: i32,
    pub image_width_px: u32,
    pub image_height_px: u32,
    pub dpi: u32,
}

impl PageGeometry {
    /// Map pixel bbox in rendered image to PDF user-space coordinates.
    pub fn px_to_pdf(&self, px: BBoxPx) -> [f32; 4] {
        let crop_width = self.crop_box[2] - self.crop_box[0];
        let crop_height = self.crop_box[3] - self.crop_box[1];
        let display_width = if matches!(self.normalized_rotation(), 90 | 270) {
            crop_height
        } else {
            crop_width
        };
        let display_height = if matches!(self.normalized_rotation(), 90 | 270) {
            crop_width
        } else {
            crop_height
        };

        let x_scale = display_width / self.image_width_px.max(1) as f32;
        let y_scale = display_height / self.image_height_px.max(1) as f32;

        let left = px.left.min(px.right) as f32 * x_scale;
        let right = px.left.max(px.right) as f32 * x_scale;
        let top = px.top.min(px.bottom) as f32 * y_scale;
        let bottom = px.top.max(px.bottom) as f32 * y_scale;

        let display_corners = [
            (left, display_height - top),
            (right, display_height - top),
            (right, display_height - bottom),
            (left, display_height - bottom),
        ];

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for (display_x, display_y) in display_corners {
            let (x, y) = self.display_to_unrotated(display_x, display_y, crop_width, crop_height);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }

        [
            self.crop_box[0] + min_x,
            self.crop_box[1] + min_y,
            self.crop_box[0] + max_x,
            self.crop_box[1] + max_y,
        ]
    }

    fn normalized_rotation(&self) -> i32 {
        self.rotation.rem_euclid(360)
    }

    fn display_to_unrotated(
        &self,
        display_x: f32,
        display_y: f32,
        width: f32,
        height: f32,
    ) -> (f32, f32) {
        match self.normalized_rotation() {
            90 => (display_y, height - display_x),
            180 => (width - display_x, height - display_y),
            270 => (width - display_y, display_x),
            _ => (display_x, display_y),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rect_close(actual: [f32; 4], expected: [f32; 4]) {
        for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
            assert!(
                (actual - expected).abs() < 0.02,
                "index {index}: actual {actual}, expected {expected}"
            );
        }
    }

    fn letter(rotation: i32, image_width_px: u32, image_height_px: u32) -> PageGeometry {
        PageGeometry {
            media_box: [0.0, 0.0, 612.0, 792.0],
            crop_box: [0.0, 0.0, 612.0, 792.0],
            rotation,
            image_width_px,
            image_height_px,
            dpi: 200,
        }
    }

    #[test]
    fn maps_letter_rotation_0() {
        let rect = letter(0, 1700, 2200).px_to_pdf(BBoxPx {
            left: 100,
            top: 100,
            right: 200,
            bottom: 150,
        });
        assert_rect_close(rect, [36.0, 738.0, 72.0, 756.0]);
    }

    #[test]
    fn maps_letter_rotation_90() {
        let rect = letter(90, 2200, 1700).px_to_pdf(BBoxPx {
            left: 100,
            top: 100,
            right: 200,
            bottom: 150,
        });
        assert_rect_close(rect, [558.0, 720.0, 576.0, 756.0]);
    }

    #[test]
    fn maps_letter_rotation_180() {
        let rect = letter(180, 1700, 2200).px_to_pdf(BBoxPx {
            left: 100,
            top: 100,
            right: 200,
            bottom: 150,
        });
        assert_rect_close(rect, [540.0, 36.0, 576.0, 54.0]);
    }

    #[test]
    fn maps_letter_rotation_270() {
        let rect = letter(270, 2200, 1700).px_to_pdf(BBoxPx {
            left: 100,
            top: 100,
            right: 200,
            bottom: 150,
        });
        assert_rect_close(rect, [36.0, 36.0, 54.0, 72.0]);
    }

    #[test]
    fn maps_a4_at_300_dpi() {
        let geometry = PageGeometry {
            media_box: [0.0, 0.0, 595.0, 842.0],
            crop_box: [0.0, 0.0, 595.0, 842.0],
            rotation: 0,
            image_width_px: 2480,
            image_height_px: 3508,
            dpi: 300,
        };
        let rect = geometry.px_to_pdf(BBoxPx {
            left: 300,
            top: 600,
            right: 900,
            bottom: 750,
        });
        assert_rect_close(rect, [71.98, 661.98, 215.93, 697.99]);
    }
}
