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

    /// Draws an entire polyline via exact minimum-distance SDF so adjacent curve segments
    /// NEVER double-blend on joints or create vertical seam artifacts.
    fn draw_smooth_polyline(
        &mut self,
        pts: &[(f32, f32)],
        thickness: f32,
        color: [u8; 4],
        alpha_scale: f32,
    ) {
        if pts.len() < 2 {
            return;
        }
        let half = thickness * 0.5;
        let reach = half + 1.2;
        let min_x = pts
            .iter()
            .map(|p| p.0)
            .fold(f32::INFINITY, f32::min)
            .sub(reach)
            .floor()
            .max(0.0) as i32;
        let max_x = pts
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .add(reach)
            .ceil()
            .min(self.w as f32 - 1.0) as i32;
        let min_y = pts
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .sub(reach)
            .floor()
            .max(0.0) as i32;
        let max_y = pts
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .add(reach)
            .ceil()
            .min(self.h as f32 - 1.0) as i32;

        for y in min_y..=max_y {
            let py = y as f32 + 0.5;
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let mut best_sq = f32::INFINITY;
                for seg in pts.windows(2) {
                    let (x0, y0) = seg[0];
                    let (x1, y1) = seg[1];
                    if px < x0.min(x1) - reach || px > x0.max(x1) + reach {
                        continue;
                    }
                    if py < y0.min(y1) - reach || py > y0.max(y1) + reach {
                        continue;
                    }
                    let dx = x1 - x0;
                    let dy = y1 - y0;
                    let len_sq = (dx * dx + dy * dy).max(1e-6);
                    let t = (((px - x0) * dx + (py - y0) * dy) / len_sq).clamp(0.0, 1.0);
                    let qx = x0 + t * dx;
                    let qy = y0 + t * dy;
                    let d_sq = (px - qx).powi(2) + (py - qy).powi(2);
                    if d_sq < best_sq {
                        best_sq = d_sq;
                    }
                }
                if best_sq <= reach * reach {
                    let dist = best_sq.sqrt();
                    let cov = (half + 0.7 - dist).clamp(0.0, 1.0);
                    if cov > 0.001 {
                        self.blend_pixel(x, y, color, cov * alpha_scale);
                    }
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

use std::ops::{Add, Sub};

/// Fritsch-Carlson monotone cubic Hermite spline sampling so average trend lines are silky
/// smooth without artificially overshooting `[y_min, y_max]`.
fn sample_monotone_cubic(coords: &[(f32, f32)], min_y: f32, max_y: f32) -> Vec<(f32, f32)> {
    let n = coords.len();
    if n < 2 {
        return coords.to_vec();
    }
    if n == 2 {
        let steps = 16usize;
        let mut out = Vec::with_capacity(steps + 1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            out.push((
                coords[0].0 + (coords[1].0 - coords[0].0) * t,
                (coords[0].1 + (coords[1].1 - coords[0].1) * t).clamp(min_y, max_y),
            ));
        }
        return out;
    }

    let mut deltas = Vec::with_capacity(n - 1);
    for k in 0..(n - 1) {
        let dx = (coords[k + 1].0 - coords[k].0).max(1e-4);
        let dy = coords[k + 1].1 - coords[k].1;
        deltas.push(dy / dx);
    }

    let mut tangents = vec![0.0f32; n];
    tangents[0] = deltas[0];
    tangents[n - 1] = deltas[n - 2];
    for k in 1..(n - 1) {
        if deltas[k - 1] * deltas[k] <= 0.0 {
            tangents[k] = 0.0;
        } else {
            tangents[k] = 0.5 * (deltas[k - 1] + deltas[k]);
        }
    }

    for k in 0..(n - 1) {
        if deltas[k].abs() < 1e-6 {
            tangents[k] = 0.0;
            tangents[k + 1] = 0.0;
        } else {
            let a = tangents[k] / deltas[k];
            let b = tangents[k + 1] / deltas[k];
            let sq = a * a + b * b;
            if sq > 9.0 {
                let tau = 3.0 / sq.sqrt();
                tangents[k] = tau * a * deltas[k];
                tangents[k + 1] = tau * b * deltas[k];
            }
        }
    }

    let steps = 16usize;
    let mut sampled = Vec::with_capacity((n - 1) * steps + 1);
    for k in 0..(n - 1) {
        let (x0, y0) = coords[k];
        let (x1, y1) = coords[k + 1];
        let h = x1 - x0;
        let m0 = tangents[k];
        let m1 = tangents[k + 1];
        let end_step = if k + 1 == n - 1 { steps } else { steps - 1 };
        for s in 0..=end_step {
            let t = s as f32 / steps as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            let sx = x0 + h * t;
            let sy = (h00 * y0 + h10 * h * m0 + h01 * y1 + h11 * h * m1).clamp(min_y, max_y);
            sampled.push((sx, sy));
        }
    }
    sampled
}

/// Renders an expressive 2x-DPI anti-aliased spline average dynamics chart with seamless
/// gradient fill, smart Y-axis zoom aligned to `y_bounds`, dashed gridlines, target average
/// reference line, and optional interactive hover crosshair.
#[allow(clippy::too_many_arguments)]
pub fn render_line_chart(
    points: &[f32],
    width: usize,
    height: usize,
    dark_mode: bool,
    line_rgb: [u8; 3],
    target_avg: Option<f32>,
    y_bounds: (f32, f32),
    hovered_index: Option<usize>,
) -> Image {
    let mut canvas = Canvas::new(width, height, [0, 0, 0, 0]);
    let pad_l = 18.0;
    let pad_r = 22.0;
    let pad_t = 18.0;
    let pad_b = 18.0;
    let plot_w = (width as f32 - pad_l - pad_r).max(10.0);
    let plot_h = (height as f32 - pad_t - pad_b).max(10.0);

    let y_min = y_bounds.0;
    let y_max = y_bounds.1;
    let y_span = (y_max - y_min).max(0.5);

    let grid_color = if dark_mode {
        [201, 197, 209, 36]
    } else {
        [72, 69, 78, 32]
    };

    // Dashed horizontal reference grid lines (4 intervals matching Y-axis labels)
    for step in 0..=4 {
        let y = pad_t + plot_h * (step as f32 / 4.0);
        if step == 4 {
            canvas.draw_line(pad_l, y, pad_l + plot_w, y, 1.5, grid_color);
        } else {
            canvas.draw_dashed_line((pad_l, y), (pad_l + plot_w, y), 1.2, (6.0, 6.0), grid_color);
        }
    }

    // Target average dashed reference line (teal accent)
    if let Some(target) = target_avg
        && (y_min..=y_max).contains(&target)
    {
        let norm = ((target - y_min) / y_span).clamp(0.0, 1.0);
        let ty = pad_t + plot_h * (1.0 - norm);
        let target_col = if dark_mode {
            [122, 215, 202, 215]
        } else {
            [0, 106, 96, 205]
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

    let sampled = sample_monotone_cubic(&coords, pad_t, pad_t + plot_h);

    // Single-pass column area gradient underneath the spline (zero vertical seam artifacts!)
    if sampled.len() >= 2 {
        let start_x = sampled.first().unwrap().0.floor().max(0.0) as i32;
        let end_x = sampled.last().unwrap().0.ceil().min(width as f32 - 1.0) as i32;
        let base_y = (pad_t + plot_h) as i32;
        let mut seg_idx = 0usize;
        for x in start_x..=end_x {
            let fx = x as f32 + 0.5;
            while seg_idx + 2 < sampled.len() && sampled[seg_idx + 1].0 < fx {
                seg_idx += 1;
            }
            let (x0, y0) = sampled[seg_idx];
            let (x1, y1) = sampled[(seg_idx + 1).min(sampled.len() - 1)];
            let t = if (x1 - x0).abs() < 0.01 {
                0.0
            } else {
                ((fx - x0) / (x1 - x0)).clamp(0.0, 1.0)
            };
            let cy = y0 + (y1 - y0) * t;
            let span_y = (base_y as f32 - cy).max(1.0);
            for y in (cy.floor() as i32)..=base_y {
                let fade = ((base_y as f32 - y as f32) / span_y).clamp(0.0, 1.0);
                let curve_fade = fade.powf(1.35);
                canvas.blend_pixel(
                    x,
                    y,
                    [line_rgb[0], line_rgb[1], line_rgb[2], 64],
                    curve_fade,
                );
            }
        }
    }

    // Soft M3 Expressive outer glow + crisp core spline
    canvas.draw_smooth_polyline(
        &sampled,
        10.0,
        [line_rgb[0], line_rgb[1], line_rgb[2], 38],
        1.0,
    );
    canvas.draw_smooth_polyline(
        &sampled,
        4.2,
        [line_rgb[0], line_rgb[1], line_rgb[2], 255],
        1.0,
    );

    // Optional hover crosshair
    if let Some(h_idx) = hovered_index
        && let Some(&(hx, hy)) = coords.get(h_idx)
    {
        let cross_col = if dark_mode {
            [230, 224, 240, 110]
        } else {
            [74, 68, 89, 95]
        };
        canvas.draw_dashed_line(
            (hx, pad_t),
            (hx, pad_t + plot_h),
            1.5,
            (5.0, 4.0),
            cross_col,
        );
        canvas.draw_circle(hx, hy, 14.0, [line_rgb[0], line_rgb[1], line_rgb[2], 45]);
    }

    // Clean M3 node markers
    let ring = if dark_mode {
        [30, 28, 34, 255]
    } else {
        [248, 245, 250, 255]
    };
    let step = (coords.len() / 24).max(1);
    for (i, &(x, y)) in coords.iter().enumerate() {
        let is_last = i == coords.len() - 1;
        let is_hovered = hovered_index == Some(i);
        if i % step == 0 || is_last || is_hovered {
            let r_outer = if is_hovered {
                10.0
            } else if is_last {
                8.5
            } else {
                6.2
            };
            let r_inner = if is_hovered {
                6.6
            } else if is_last {
                5.4
            } else {
                3.9
            };
            canvas.draw_circle(x, y, r_outer, ring);
            canvas.draw_circle(x, y, r_inner, [line_rgb[0], line_rgb[1], line_rgb[2], 255]);
        }
    }

    canvas.into_image()
}

/// Shortest signed angular distance to arc `[a0, a1]` (angles in `[0, TAU)` with `a1` possibly `> TAU`),
/// returning the SDF to an annular rounded-cap pill segment.
fn rounded_arc_sdf(px: f32, py: f32, mid_r: f32, cap_r: f32, start_a: f32, end_a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let half_pi = std::f32::consts::FRAC_PI_2;
    let r = (px * px + py * py).sqrt();
    let mut ang = py.atan2(px) + half_pi;
    if ang < 0.0 {
        ang += tau;
    }

    let in_arc = if end_a <= tau {
        ang >= start_a && ang <= end_a
    } else {
        ang >= start_a || ang <= (end_a - tau)
    };

    if in_arc {
        return (r - mid_r).abs() - cap_r;
    }

    // Distance to rounded cap circles at start_a and end_a
    let theta0 = start_a - half_pi;
    let theta1 = end_a - half_pi;
    let c0x = mid_r * theta0.cos();
    let c0y = mid_r * theta0.sin();
    let c1x = mid_r * theta1.cos();
    let c1y = mid_r * theta1.sin();

    let d0 = ((px - c0x).powi(2) + (py - c0y).powi(2)).sqrt();
    let d1 = ((px - c1x).powi(2) + (py - c1y).powi(2)).sqrt();
    d0.min(d1) - cap_r
}

/// Renders an anti-aliased Material Design 3 Expressive rounded-cap donut chart
/// with a subtle tonal track ring and clean spacing between segments.
pub fn render_donut_chart(segments: &[(usize, [u8; 3])], size: usize, dark_mode: bool) -> Image {
    let mut canvas = Canvas::new(size, size, [0, 0, 0, 0]);
    let total: usize = segments.iter().map(|(c, _)| *c).sum();
    let cx = size as f32 * 0.5;
    let cy = size as f32 * 0.5;
    let outer_r = size as f32 * 0.43;
    let inner_r = size as f32 * 0.295;
    let mid_r = (outer_r + inner_r) * 0.5;
    let cap_r = (outer_r - inner_r) * 0.5;

    let track_ring = if dark_mode {
        [68, 65, 78, 120]
    } else {
        [218, 214, 226, 170]
    };

    // Always draw crisp tonal track ring underneath so gaps look purposeful
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d >= inner_r - 1.2 && d <= outer_r + 1.2 {
                let sdf = (d - mid_r).abs() - cap_r;
                let cov = (0.7 - sdf).clamp(0.0, 1.0);
                canvas.blend_pixel(x as i32, y as i32, track_ring, cov);
            }
        }
    }

    if total == 0 {
        return canvas.into_image();
    }

    let tau = std::f32::consts::TAU;
    let non_zero = segments.iter().filter(|(c, _)| *c > 0).count();
    let mut arcs: Vec<(f32, f32, [u8; 3])> = Vec::new();
    let mut acc = 0.0f32;

    // Each rounded cap extends `cap_r / mid_r` rad beyond its endpoint; add clean gap_px.
    let gap_rad = if non_zero > 1 {
        (2.0 * cap_r + 12.0) / mid_r
    } else {
        0.0
    };

    for &(count, rgb) in segments {
        if count == 0 {
            continue;
        }
        let sweep = (count as f32 / total as f32) * tau;
        if non_zero == 1 {
            // Full circle: draw closed ring smoothly
            for y in 0..size {
                for x in 0..size {
                    let dx = x as f32 + 0.5 - cx;
                    let dy = y as f32 + 0.5 - cy;
                    let sdf = ((dx * dx + dy * dy).sqrt() - mid_r).abs() - cap_r;
                    let cov = (0.7 - sdf).clamp(0.0, 1.0);
                    if cov > 0.001 {
                        canvas.blend_pixel(x as i32, y as i32, [rgb[0], rgb[1], rgb[2], 255], cov);
                    }
                }
            }
            return canvas.into_image();
        }
        let mid_a = acc + sweep * 0.5;
        let trim = (gap_rad * 0.5).min(sweep * 0.49);
        let a0 = (acc + trim).min(mid_a);
        let a1 = (acc + sweep - trim).max(mid_a);
        arcs.push((a0, a1, rgb));
        acc += sweep;
    }

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d < inner_r - 1.5 || d > outer_r + 1.5 {
                continue;
            }
            for &(a0, a1, rgb) in &arcs {
                let sdf = rounded_arc_sdf(dx, dy, mid_r, cap_r, a0, a1);
                if sdf < 1.2 {
                    let cov = (0.7 - sdf).clamp(0.0, 1.0);
                    canvas.blend_pixel(x as i32, y as i32, [rgb[0], rgb[1], rgb[2], 255], cov);
                }
            }
        }
    }

    canvas.into_image()
}
