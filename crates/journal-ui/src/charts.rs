use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

struct Canvas {
    w: usize,
    h: usize,
    buf: Vec<u8>,
}

impl Canvas {
    fn new(w: usize, h: usize, bg: [u8; 4]) -> Self {
        let mut buf = vec![0u8; w * h * 4];
        for px in buf.as_chunks_mut::<4>().0 {
            px.copy_from_slice(&bg);
        }
        Self { w, h, buf }
    }

    fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 4], alpha_scale: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let a = (color[3] as f32 / 255.0) * alpha_scale.clamp(0.0, 1.0);
        if a <= 0.001 {
            return;
        }
        let idx = (y as usize * self.w + x as usize) * 4;
        let dst = &mut self.buf[idx..idx + 4];
        let inv = 1.0 - a;
        dst[0] = (color[0] as f32 * a + dst[0] as f32 * inv).round() as u8;
        dst[1] = (color[1] as f32 * a + dst[1] as f32 * inv).round() as u8;
        dst[2] = (color[2] as f32 * a + dst[2] as f32 * inv).round() as u8;
        dst[3] = ((a + (dst[3] as f32 / 255.0) * inv).min(1.0) * 255.0).round() as u8;
    }

    fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: [u8; 4]) {
        let min_x = (x0.min(x1) - thickness - 2.0).floor().max(0.0) as i32;
        let max_x = (x0.max(x1) + thickness + 2.0)
            .ceil()
            .min(self.w as f32 - 1.0) as i32;
        let min_y = (y0.min(y1) - thickness - 2.0).floor().max(0.0) as i32;
        let max_y = (y0.max(y1) + thickness + 2.0)
            .ceil()
            .min(self.h as f32 - 1.0) as i32;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len_sq = (dx * dx + dy * dy).max(1e-6);
        let half = thickness * 0.5;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let t = (((px - x0) * dx + (py - y0) * dy) / len_sq).clamp(0.0, 1.0);
                let proj_x = x0 + t * dx;
                let proj_y = y0 + t * dy;
                let dist = ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt();
                if dist <= half + 1.0 {
                    let cov = (half + 0.7 - dist).clamp(0.0, 1.0);
                    self.blend_pixel(x, y, color, cov);
                }
            }
        }
    }

    fn draw_dashed_line(
        &mut self,
        p0: (f32, f32),
        p1: (f32, f32),
        thickness: f32,
        dash_gap: (f32, f32),
        color: [u8; 4],
    ) {
        let (x0, y0) = p0;
        let (x1, y1) = p1;
        let (dash, gap) = dash_gap;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let total = (dx * dx + dy * dy).sqrt();
        if total < 1.0 {
            return;
        }
        let ux = dx / total;
        let uy = dy / total;
        let mut dist = 0.0;
        while dist < total {
            let end = (dist + dash).min(total);
            self.draw_line(
                x0 + ux * dist,
                y0 + uy * dist,
                x0 + ux * end,
                y0 + uy * end,
                thickness,
                color,
            );
            dist += dash + gap;
        }
    }

    fn draw_circle(&mut self, cx: f32, cy: f32, radius: f32, color: [u8; 4]) {
        let min_x = (cx - radius - 2.0).floor().max(0.0) as i32;
        let max_x = (cx + radius + 2.0).ceil().min(self.w as f32 - 1.0) as i32;
        let min_y = (cy - radius - 2.0).floor().max(0.0) as i32;
        let max_y = (cy + radius + 2.0).ceil().min(self.h as f32 - 1.0) as i32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dist = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                if dist <= radius + 1.0 {
                    let cov = (radius + 0.6 - dist).clamp(0.0, 1.0);
                    self.blend_pixel(x, y, color, cov);
                }
            }
        }
    }

    fn into_image(self) -> Image {
        let pixel_buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            &self.buf,
            self.w as u32,
            self.h as u32,
        );
        Image::from_rgba8(pixel_buf)
    }
}

/// Renders a crisp 2x-DPI cumulative average dynamics line chart with translucent area fill,
/// smart Y-axis zoom, dashed horizontal gridlines, and optional target average reference line.
pub fn render_line_chart(
    points: &[f32],
    width: usize,
    height: usize,
    dark_mode: bool,
    line_rgb: [u8; 3],
    target_avg: Option<f32>,
) -> Image {
    let bg = [0, 0, 0, 0];
    let mut canvas = Canvas::new(width, height, bg);
    let pad_l = 16.0;
    let pad_r = 20.0;
    let pad_t = 18.0;
    let pad_b = 18.0;
    let plot_w = (width as f32 - pad_l - pad_r).max(10.0);
    let plot_h = (height as f32 - pad_t - pad_b).max(10.0);

    // Smart dynamic Y-axis range: zooms into [3.0, 5.0] when all grades >= 3.3, else [1.0, 5.0]
    let min_val = points
        .iter()
        .copied()
        .chain(target_avg)
        .fold(5.0f32, f32::min);
    let y_min = if min_val >= 3.3 { 3.0f32 } else { 1.0f32 };
    let y_max = 5.0f32;
    let y_span = (y_max - y_min).max(0.5);

    let grid_color = if dark_mode {
        [201, 197, 209, 34]
    } else {
        [72, 69, 78, 30]
    };

    // Dashed horizontal reference grid lines (4 intervals)
    for step in 0..=4 {
        let y = pad_t + plot_h * (step as f32 / 4.0);
        if step == 4 {
            canvas.draw_line(pad_l, y, pad_l + plot_w, y, 1.4, grid_color);
        } else {
            canvas.draw_dashed_line((pad_l, y), (pad_l + plot_w, y), 1.2, (6.0, 6.0), grid_color);
        }
    }

    // Optional target average reference line (teal grade-4 accent)
    if let Some(target) = target_avg
        && (y_min..=y_max).contains(&target)
    {
        let norm = ((target - y_min) / y_span).clamp(0.0, 1.0);
        let ty = pad_t + plot_h * (1.0 - norm);
        let target_col = if dark_mode {
            [122, 215, 202, 220]
        } else {
            [0, 106, 96, 210]
        };
        canvas.draw_dashed_line(
            (pad_l, ty),
            (pad_l + plot_w, ty),
            2.2,
            (10.0, 7.0),
            target_col,
        );
    }

    if points.is_empty() {
        return canvas.into_image();
    }

    let coords: Vec<(f32, f32)> = points
        .iter()
        .enumerate()
        .map(|(i, &val)| {
            let x = if points.len() == 1 {
                pad_l + plot_w * 0.5
            } else {
                pad_l + plot_w * (i as f32 / (points.len() - 1) as f32)
            };
            let norm = ((val - y_min) / y_span).clamp(0.0, 1.0);
            let y = pad_t + plot_h * (1.0 - norm);
            (x, y)
        })
        .collect();

    // Subtle vertical gradient fill beneath curve
    if coords.len() >= 2 {
        let base_y = (pad_t + plot_h) as i32;
        for seg in coords.windows(2) {
            let (x0, y0) = seg[0];
            let (x1, y1) = seg[1];
            let sx = x0.floor().max(0.0) as i32;
            let ex = x1.ceil().min(width as f32 - 1.0) as i32;
            for x in sx..=ex {
                let t = if (x1 - x0).abs() < 0.01 {
                    0.0
                } else {
                    ((x as f32 - x0) / (x1 - x0)).clamp(0.0, 1.0)
                };
                let cy = y0 + (y1 - y0) * t;
                for y in (cy.ceil() as i32)..=base_y {
                    let fade = 1.0 - ((y as f32 - cy) / (base_y as f32 - cy).max(1.0));
                    canvas.blend_pixel(x, y, [line_rgb[0], line_rgb[1], line_rgb[2], 46], fade);
                }
            }
        }
    }

    // Smooth crisp curve segments
    for seg in coords.windows(2) {
        canvas.draw_line(
            seg[0].0,
            seg[0].1,
            seg[1].0,
            seg[1].1,
            4.0,
            [line_rgb[0], line_rgb[1], line_rgb[2], 255],
        );
    }

    // Data point nodes (larger highlight ring on final latest value)
    let ring = if dark_mode {
        [28, 27, 31, 255]
    } else {
        [245, 241, 246, 255]
    };
    let step = (coords.len() / 20).max(1);
    for (i, &(x, y)) in coords.iter().enumerate() {
        let is_last = i == coords.len() - 1;
        if i % step == 0 || is_last {
            let r_outer = if is_last { 8.0 } else { 6.0 };
            let r_inner = if is_last { 5.4 } else { 4.0 };
            canvas.draw_circle(x, y, r_outer, ring);
            canvas.draw_circle(x, y, r_inner, [line_rgb[0], line_rgb[1], line_rgb[2], 255]);
        }
    }

    canvas.into_image()
}

/// Renders a crisp anti-aliased Material 3 donut chart for grade structure breakdown.
pub fn render_donut_chart(segments: &[(usize, [u8; 3])], size: usize, dark_mode: bool) -> Image {
    let mut canvas = Canvas::new(size, size, [0, 0, 0, 0]);
    let total: usize = segments.iter().map(|(c, _)| *c).sum();
    let cx = size as f32 * 0.5;
    let cy = size as f32 * 0.5;
    let outer_r = size as f32 * 0.44;
    let inner_r = size as f32 * 0.27;

    let empty_ring = if dark_mode {
        [70, 74, 86, 180]
    } else {
        [218, 222, 232, 220]
    };

    if total == 0 {
        for y in 0..size {
            for x in 0..size {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                if d >= inner_r - 1.0 && d <= outer_r + 1.0 {
                    let cov = (outer_r + 0.5 - d).min(d - inner_r + 0.5).clamp(0.0, 1.0);
                    canvas.blend_pixel(x as i32, y as i32, empty_ring, cov);
                }
            }
        }
        return canvas.into_image();
    }

    let mut ranges: Vec<(f32, f32, [u8; 3])> = Vec::new();
    let mut acc = 0.0f32;
    let tau = std::f32::consts::TAU;
    let gap = if segments.iter().filter(|(c, _)| *c > 0).count() > 1 {
        0.025
    } else {
        0.0
    };
    for &(count, rgb) in segments {
        if count == 0 {
            continue;
        }
        let sweep = (count as f32 / total as f32) * tau;
        ranges.push((acc + gap * 0.5, acc + sweep - gap * 0.5, rgb));
        acc += sweep;
    }

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d < inner_r - 1.0 || d > outer_r + 1.0 {
                continue;
            }
            let radial_cov = (outer_r + 0.6 - d).min(d - inner_r + 0.6).clamp(0.0, 1.0);
            let mut angle = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
            if angle < 0.0 {
                angle += tau;
            }
            for &(a0, a1, rgb) in &ranges {
                if angle >= a0 && angle <= a1 {
                    canvas.blend_pixel(
                        x as i32,
                        y as i32,
                        [rgb[0], rgb[1], rgb[2], 255],
                        radial_cov,
                    );
                    break;
                }
            }
        }
    }

    canvas.into_image()
}
