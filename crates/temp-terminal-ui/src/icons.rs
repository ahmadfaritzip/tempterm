use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

pub struct Icons;

impl Icons {
    /// TempTerm rebrand icon: sleek glowing terminal shield with TT monogram
    pub fn draw_logo(painter: &Painter, rect: Rect, accent: Color32, text_color: Color32) {
        let center = rect.center();
        let size = rect.width().min(rect.height()) * 0.9;
        let r = size * 0.5;

        // Background rounded diamond shield
        let bg_rect = Rect::from_center_size(center, Vec2::splat(size));
        painter.rect_filled(
            bg_rect,
            6.0,
            Color32::from_rgba_premultiplied(accent.r() / 4, accent.g() / 4, accent.b() / 4, 180),
        );
        painter.rect_stroke(bg_rect, 6.0, Stroke::new(1.5_f32, accent));

        // TT monogram inside
        let left_t_bar = [
            Pos2::new(center.x - r * 0.6, center.y - r * 0.4),
            Pos2::new(center.x + r * 0.1, center.y - r * 0.4),
        ];
        let left_t_stem = [
            Pos2::new(center.x - r * 0.25, center.y - r * 0.4),
            Pos2::new(center.x - r * 0.25, center.y + r * 0.5),
        ];
        painter.line_segment(left_t_bar, Stroke::new(2.5_f32, text_color));
        painter.line_segment(left_t_stem, Stroke::new(2.5_f32, text_color));

        let right_t_bar = [
            Pos2::new(center.x - r * 0.05, center.y - r * 0.15),
            Pos2::new(center.x + r * 0.65, center.y - r * 0.15),
        ];
        let right_t_stem = [
            Pos2::new(center.x + r * 0.3, center.y - r * 0.15),
            Pos2::new(center.x + r * 0.3, center.y + r * 0.6),
        ];
        painter.line_segment(right_t_bar, Stroke::new(2.5_f32, accent));
        painter.line_segment(right_t_stem, Stroke::new(2.5_f32, accent));
    }

    pub fn draw_terminal(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.5_f32, color);
        let m = rect.width() * 0.2;
        let p1 = Pos2::new(rect.min.x + m, rect.min.y + m);
        let p2 = Pos2::new(rect.min.x + rect.width() * 0.48, rect.center().y);
        let p3 = Pos2::new(rect.min.x + m, rect.max.y - m);
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], stroke);

        let p_line1 = Pos2::new(rect.min.x + rect.width() * 0.52, rect.max.y - m);
        let p_line2 = Pos2::new(rect.max.x - m, rect.max.y - m);
        painter.line_segment([p_line1, p_line2], stroke);
    }

    pub fn draw_folder(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let pad = rect.width() * 0.15;
        let inner = rect.shrink(pad);
        let top_tab_w = inner.width() * 0.4;
        let tab_h = inner.height() * 0.28;

        let points = vec![
            Pos2::new(inner.min.x, inner.min.y + tab_h),
            Pos2::new(inner.min.x + top_tab_w * 0.7, inner.min.y + tab_h),
            Pos2::new(inner.min.x + top_tab_w, inner.min.y),
            Pos2::new(inner.max.x, inner.min.y),
            Pos2::new(inner.max.x, inner.max.y),
            Pos2::new(inner.min.x, inner.max.y),
            Pos2::new(inner.min.x, inner.min.y + tab_h),
        ];
        for i in 0..points.len() - 1 {
            painter.line_segment([points[i], points[i + 1]], stroke);
        }
    }

    pub fn draw_folder_plus(painter: &Painter, rect: Rect, color: Color32) {
        Self::draw_folder(painter, rect, color);
        let c = rect.center();
        let stroke = Stroke::new(1.3_f32, color);
        let len = rect.width() * 0.16;
        painter.line_segment(
            [Pos2::new(c.x, c.y - len), Pos2::new(c.x, c.y + len)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(c.x - len, c.y), Pos2::new(c.x + len, c.y)],
            stroke,
        );
    }

    pub fn draw_folder_git(painter: &Painter, rect: Rect, color: Color32) {
        Self::draw_folder(painter, rect, color);
        let stroke = Stroke::new(1.2_f32, color);
        let c = Pos2::new(
            rect.min.x + rect.width() * 0.65,
            rect.min.y + rect.height() * 0.68,
        );
        painter.circle_stroke(c, 2.0, stroke);
    }

    pub fn draw_git_branch(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let r = 2.4;
        let left_x = rect.min.x + rect.width() * 0.32;
        let right_x = rect.min.x + rect.width() * 0.68;
        let top_y = rect.min.y + rect.height() * 0.28;
        let bot_y = rect.min.y + rect.height() * 0.72;

        painter.circle_stroke(Pos2::new(left_x, bot_y), r, stroke);
        painter.circle_stroke(Pos2::new(left_x, top_y), r, stroke);
        painter.circle_stroke(Pos2::new(right_x, top_y + 1.5), r, stroke);

        // Trunk
        painter.line_segment(
            [Pos2::new(left_x, top_y + r), Pos2::new(left_x, bot_y - r)],
            stroke,
        );
        // Branch curve
        let mid_y = (top_y + bot_y) * 0.55;
        painter.line_segment(
            [
                Pos2::new(left_x, mid_y),
                Pos2::new(right_x - 3.0, top_y + r + 2.0),
            ],
            stroke,
        );
    }

    pub fn draw_bot(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let head = rect.shrink(rect.width() * 0.22);
        painter.rect_stroke(head, 3.0, stroke);

        // Antenna
        let ant_bot = Pos2::new(head.center().x, head.min.y);
        let ant_top = Pos2::new(head.center().x, head.min.y - 3.0);
        painter.line_segment([ant_bot, ant_top], stroke);
        painter.circle_filled(ant_top, 1.5, color);

        // Eyes
        let eye_y = head.min.y + head.height() * 0.42;
        let eye_offset = head.width() * 0.22;
        painter.circle_filled(Pos2::new(head.center().x - eye_offset, eye_y), 1.5, color);
        painter.circle_filled(Pos2::new(head.center().x + eye_offset, eye_y), 1.5, color);

        // Mouth
        let m_y = head.min.y + head.height() * 0.75;
        painter.line_segment(
            [
                Pos2::new(head.center().x - eye_offset * 0.7, m_y),
                Pos2::new(head.center().x + eye_offset * 0.7, m_y),
            ],
            stroke,
        );
    }

    pub fn draw_check(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.8_f32, color);
        let p1 = Pos2::new(rect.min.x + rect.width() * 0.2, rect.center().y);
        let p2 = Pos2::new(
            rect.min.x + rect.width() * 0.45,
            rect.max.y - rect.height() * 0.25,
        );
        let p3 = Pos2::new(
            rect.max.x - rect.width() * 0.2,
            rect.min.y + rect.height() * 0.25,
        );
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], stroke);
    }

    pub fn draw_alert(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let p1 = Pos2::new(rect.center().x, rect.min.y + rect.height() * 0.15);
        let p2 = Pos2::new(
            rect.max.x - rect.width() * 0.15,
            rect.max.y - rect.height() * 0.15,
        );
        let p3 = Pos2::new(
            rect.min.x + rect.width() * 0.15,
            rect.max.y - rect.height() * 0.15,
        );
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], stroke);
        painter.line_segment([p3, p1], stroke);

        // Exclamation mark
        let dot_pos = Pos2::new(rect.center().x, p2.y - 3.5);
        painter.circle_filled(dot_pos, 1.2, color);
        painter.line_segment(
            [
                Pos2::new(rect.center().x, p1.y + 4.5),
                Pos2::new(rect.center().x, dot_pos.y - 2.5),
            ],
            Stroke::new(1.5_f32, color),
        );
    }

    pub fn draw_spinner(painter: &Painter, rect: Rect, color: Color32, angle: f32) {
        let center = rect.center();
        let r = rect.width() * 0.38;
        let stroke = Stroke::new(1.8_f32, color);

        // Draw an animated arc
        let steps = 16;
        let arc_span = std::f32::consts::PI * 1.4;
        let start_angle = angle;
        let mut prev = Pos2::new(
            center.x + r * (start_angle).cos(),
            center.y + r * (start_angle).sin(),
        );

        for i in 1..=steps {
            let a = start_angle + (arc_span * (i as f32) / (steps as f32));
            let curr = Pos2::new(center.x + r * a.cos(), center.y + r * a.sin());
            painter.line_segment([prev, curr], stroke);
            prev = curr;
        }
    }

    pub fn draw_search(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let r = rect.width() * 0.26;
        let center = Pos2::new(
            rect.min.x + rect.width() * 0.42,
            rect.min.y + rect.height() * 0.42,
        );
        painter.circle_stroke(center, r, stroke);

        let handle_start = Pos2::new(center.x + r * 0.7, center.y + r * 0.7);
        let handle_end = Pos2::new(
            rect.max.x - rect.width() * 0.18,
            rect.max.y - rect.height() * 0.18,
        );
        painter.line_segment([handle_start, handle_end], stroke);
    }

    pub fn draw_split_right(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let inner = rect.shrink(rect.width() * 0.18);
        painter.rect_stroke(inner, 2.0, stroke);
        painter.line_segment(
            [
                Pos2::new(inner.center().x, inner.min.y),
                Pos2::new(inner.center().x, inner.max.y),
            ],
            stroke,
        );
    }

    pub fn draw_split_v(painter: &Painter, rect: Rect, color: Color32) {
        Self::draw_split_right(painter, rect, color);
    }

    pub fn draw_split_down(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let inner = rect.shrink(rect.width() * 0.18);
        painter.rect_stroke(inner, 2.0, stroke);
        painter.line_segment(
            [
                Pos2::new(inner.min.x, inner.center().y),
                Pos2::new(inner.max.x, inner.center().y),
            ],
            stroke,
        );
    }

    pub fn draw_split_h(painter: &Painter, rect: Rect, color: Color32) {
        Self::draw_split_down(painter, rect, color);
    }

    pub fn draw_agent(painter: &Painter, rect: Rect, color: Color32) {
        Self::draw_bot(painter, rect, color);
    }

    pub fn draw_process(painter: &Painter, rect: Rect, color: Color32) {
        Self::draw_radio(painter, rect, color);
    }

    pub fn draw_maximize(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let inner = rect.shrink(rect.width() * 0.22);
        painter.rect_stroke(inner, 2.0, stroke);
    }

    pub fn draw_close(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.5_f32, color);
        let pad = rect.width() * 0.30;
        let r = rect.shrink(pad);
        painter.line_segment([r.left_top(), r.right_bottom()], stroke);
        painter.line_segment([r.right_top(), r.left_bottom()], stroke);
    }

    pub fn draw_plus(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.5_f32, color);
        let pad = rect.width() * 0.25;
        let r = rect.shrink(pad);
        painter.line_segment(
            [Pos2::new(r.center().x, r.min.y), Pos2::new(r.center().x, r.max.y)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(r.min.x, r.center().y), Pos2::new(r.max.x, r.center().y)],
            stroke,
        );
    }

    pub fn draw_gear(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let c = rect.center();
        let r_inner = rect.width() * 0.18;
        let r_outer = rect.width() * 0.32;
        painter.circle_stroke(c, r_inner, stroke);

        for i in 0..6 {
            let a = i as f32 * std::f32::consts::PI / 3.0;
            let p1 = Pos2::new(c.x + r_inner * a.cos(), c.y + r_inner * a.sin());
            let p2 = Pos2::new(c.x + r_outer * a.cos(), c.y + r_outer * a.sin());
            painter.line_segment([p1, p2], stroke);
        }
    }

    pub fn draw_globe(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let c = rect.center();
        let r = rect.width() * 0.34;
        painter.circle_stroke(c, r, stroke);
        painter.line_segment([Pos2::new(c.x - r, c.y), Pos2::new(c.x + r, c.y)], stroke);
        painter.line_segment([Pos2::new(c.x, c.y - r), Pos2::new(c.x, c.y + r)], stroke);
    }

    pub fn draw_keyboard(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let inner = rect.shrink(rect.width() * 0.16);
        painter.rect_stroke(inner, 3.0, stroke);

        // Keys
        let y1 = inner.min.y + inner.height() * 0.38;
        let y2 = inner.min.y + inner.height() * 0.68;
        let w = inner.width();

        painter.circle_filled(Pos2::new(inner.min.x + w * 0.25, y1), 1.0, color);
        painter.circle_filled(Pos2::new(inner.min.x + w * 0.50, y1), 1.0, color);
        painter.circle_filled(Pos2::new(inner.min.x + w * 0.75, y1), 1.0, color);

        painter.line_segment(
            [
                Pos2::new(inner.min.x + w * 0.3, y2),
                Pos2::new(inner.max.x - w * 0.3, y2),
            ],
            Stroke::new(1.3_f32, color),
        );
    }

    pub fn draw_panel_left_close(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let inner = rect.shrink(rect.width() * 0.16);
        painter.rect_stroke(inner, 2.0, stroke);
        let div_x = inner.min.x + inner.width() * 0.35;
        painter.line_segment(
            [Pos2::new(div_x, inner.min.y), Pos2::new(div_x, inner.max.y)],
            stroke,
        );

        // Arrow < in right half
        let arrow_tip = Pos2::new(inner.min.x + inner.width() * 0.58, inner.center().y);
        let arrow_top = Pos2::new(
            inner.min.x + inner.width() * 0.76,
            inner.center().y - inner.height() * 0.22,
        );
        let arrow_bot = Pos2::new(
            inner.min.x + inner.width() * 0.76,
            inner.center().y + inner.height() * 0.22,
        );
        painter.line_segment([arrow_top, arrow_tip], stroke);
        painter.line_segment([arrow_tip, arrow_bot], stroke);
    }

    pub fn draw_panel_right(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let inner = rect.shrink(rect.width() * 0.16);
        painter.rect_stroke(inner, 2.0, stroke);
        let div_x = inner.min.x + inner.width() * 0.65;
        painter.line_segment(
            [Pos2::new(div_x, inner.min.y), Pos2::new(div_x, inner.max.y)],
            stroke,
        );
    }

    pub fn draw_info(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let c = rect.center();
        let r = rect.width() * 0.38;
        painter.circle_stroke(c, r, stroke);

        // Dot of 'i'
        let dot_pos = Pos2::new(c.x, c.y - r * 0.45);
        painter.circle_filled(dot_pos, 1.1, color);

        // Stem of 'i'
        let stem_top = Pos2::new(c.x, c.y - r * 0.12);
        let stem_bot = Pos2::new(c.x, c.y + r * 0.52);
        painter.line_segment([stem_top, stem_bot], Stroke::new(1.4_f32, color));
    }

    pub fn draw_refresh(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let c = rect.center();
        let r = rect.width() * 0.30;
        let steps = 16;
        let start_a = -0.15 * std::f32::consts::PI;
        let end_a = 1.45 * std::f32::consts::PI;

        let mut prev = Pos2::new(c.x + r * start_a.cos(), c.y + r * start_a.sin());
        for i in 1..=steps {
            let a = start_a + (end_a - start_a) * (i as f32 / steps as f32);
            let curr = Pos2::new(c.x + r * a.cos(), c.y + r * a.sin());
            painter.line_segment([prev, curr], stroke);
            prev = curr;
        }

        // Arrow head at start_a
        let p = Pos2::new(c.x + r * start_a.cos(), c.y + r * start_a.sin());
        painter.line_segment([p, Pos2::new(p.x - 2.5, p.y - 2.5)], stroke);
        painter.line_segment([p, Pos2::new(p.x + 3.0, p.y - 1.0)], stroke);
    }

    pub fn draw_radio(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let c = rect.center();
        // Central dot
        painter.circle_filled(c, 1.8, color);

        // Inner left and right waves
        let r1 = rect.width() * 0.22;
        painter.line_segment(
            [
                Pos2::new(c.x - r1, c.y - r1 * 0.7),
                Pos2::new(c.x - r1 * 1.3, c.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(c.x - r1 * 1.3, c.y),
                Pos2::new(c.x - r1, c.y + r1 * 0.7),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(c.x + r1, c.y - r1 * 0.7),
                Pos2::new(c.x + r1 * 1.3, c.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(c.x + r1 * 1.3, c.y),
                Pos2::new(c.x + r1, c.y + r1 * 0.7),
            ],
            stroke,
        );

        // Outer left and right waves
        let r2 = rect.width() * 0.38;
        painter.line_segment(
            [
                Pos2::new(c.x - r2 * 0.8, c.y - r2 * 0.8),
                Pos2::new(c.x - r2 * 1.1, c.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(c.x - r2 * 1.1, c.y),
                Pos2::new(c.x - r2 * 0.8, c.y + r2 * 0.8),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(c.x + r2 * 0.8, c.y - r2 * 0.8),
                Pos2::new(c.x + r2 * 1.1, c.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(c.x + r2 * 1.1, c.y),
                Pos2::new(c.x + r2 * 0.8, c.y + r2 * 0.8),
            ],
            stroke,
        );
    }

    pub fn draw_chevron_right(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let c = rect.center();
        let sz = 3.0;
        let p1 = Pos2::new(c.x - sz * 0.6, c.y - sz);
        let p2 = Pos2::new(c.x + sz * 0.6, c.y);
        let p3 = Pos2::new(c.x - sz * 0.6, c.y + sz);
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], stroke);
    }

    pub fn draw_chevron_down(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let c = rect.center();
        let sz = 3.0;
        let p1 = Pos2::new(c.x - sz, c.y - sz * 0.6);
        let p2 = Pos2::new(c.x, c.y + sz * 0.6);
        let p3 = Pos2::new(c.x + sz, c.y - sz * 0.6);
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], stroke);
    }

    pub fn draw_file_text(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let inner = rect.shrink(rect.width() * 0.16);
        let fold = inner.width() * 0.30;

        let p_tl = inner.left_top();
        let p_fold_start = Pos2::new(inner.max.x - fold, inner.min.y);
        let p_fold_mid = Pos2::new(inner.max.x - fold, inner.min.y + fold);
        let p_fold_end = Pos2::new(inner.max.x, inner.min.y + fold);
        let p_br = inner.right_bottom();
        let p_bl = inner.left_bottom();

        painter.line_segment([p_tl, p_fold_start], stroke);
        painter.line_segment([p_fold_start, p_fold_end], stroke);
        painter.line_segment([p_fold_end, p_br], stroke);
        painter.line_segment([p_br, p_bl], stroke);
        painter.line_segment([p_bl, p_tl], stroke);

        painter.line_segment([p_fold_start, p_fold_mid], stroke);
        painter.line_segment([p_fold_mid, p_fold_end], stroke);

        // Lines inside
        let line_y1 = inner.min.y + inner.height() * 0.48;
        let line_y2 = inner.min.y + inner.height() * 0.70;
        let lx1 = inner.min.x + inner.width() * 0.22;
        let lx2 = inner.max.x - inner.width() * 0.22;
        let l_stroke = Stroke::new(1.0_f32, color);
        painter.line_segment([Pos2::new(lx1, line_y1), Pos2::new(lx2, line_y1)], l_stroke);
        painter.line_segment(
            [
                Pos2::new(lx1, line_y2),
                Pos2::new(lx1 + (lx2 - lx1) * 0.65, line_y2),
            ],
            l_stroke,
        );
    }

    pub fn draw_file_plus(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let inner = rect.shrink(rect.width() * 0.16);
        let fold = inner.width() * 0.30;

        let p_tl = inner.left_top();
        let p_fold_start = Pos2::new(inner.max.x - fold, inner.min.y);
        let p_fold_mid = Pos2::new(inner.max.x - fold, inner.min.y + fold);
        let p_fold_end = Pos2::new(inner.max.x, inner.min.y + fold);
        let p_br = inner.right_bottom();
        let p_bl = inner.left_bottom();

        painter.line_segment([p_tl, p_fold_start], stroke);
        painter.line_segment([p_fold_start, p_fold_end], stroke);
        painter.line_segment([p_fold_end, p_br], stroke);
        painter.line_segment([p_br, p_bl], stroke);
        painter.line_segment([p_bl, p_tl], stroke);

        painter.line_segment([p_fold_start, p_fold_mid], stroke);
        painter.line_segment([p_fold_mid, p_fold_end], stroke);

        // Plus in bottom center
        let c = Pos2::new(inner.center().x, inner.min.y + inner.height() * 0.62);
        let plus_len = inner.width() * 0.18;
        painter.line_segment(
            [Pos2::new(c.x, c.y - plus_len), Pos2::new(c.x, c.y + plus_len)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(c.x - plus_len, c.y), Pos2::new(c.x + plus_len, c.y)],
            stroke,
        );
    }

    pub fn draw_image(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let inner = rect.shrink(rect.width() * 0.16);
        painter.rect_stroke(inner, 2.0, stroke);

        let sun = Pos2::new(
            inner.min.x + inner.width() * 0.72,
            inner.min.y + inner.height() * 0.32,
        );
        painter.circle_stroke(sun, 1.6, stroke);

        let p1 = Pos2::new(inner.min.x + 2.0, inner.max.y - 2.0);
        let p2 = Pos2::new(
            inner.min.x + inner.width() * 0.42,
            inner.min.y + inner.height() * 0.52,
        );
        let p3 = Pos2::new(inner.min.x + inner.width() * 0.64, inner.max.y - 3.0);
        let p4 = Pos2::new(
            inner.min.x + inner.width() * 0.78,
            inner.min.y + inner.height() * 0.64,
        );
        let p5 = Pos2::new(inner.max.x - 2.0, inner.max.y - 2.0);
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], stroke);
        painter.line_segment([p3, p4], stroke);
        painter.line_segment([p4, p5], stroke);
    }

    pub fn draw_arrow_down(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let cx = rect.center().x;
        let top_y = rect.min.y + rect.height() * 0.2;
        let bot_y = rect.max.y - rect.height() * 0.2;
        let barb = rect.width() * 0.26;
        painter.line_segment([Pos2::new(cx, top_y), Pos2::new(cx, bot_y)], stroke);
        painter.line_segment(
            [Pos2::new(cx - barb, bot_y - barb), Pos2::new(cx, bot_y)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(cx + barb, bot_y - barb), Pos2::new(cx, bot_y)],
            stroke,
        );
    }

    pub fn draw_arrow_up(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.4_f32, color);
        let cx = rect.center().x;
        let top_y = rect.min.y + rect.height() * 0.2;
        let bot_y = rect.max.y - rect.height() * 0.2;
        let barb = rect.width() * 0.26;
        painter.line_segment([Pos2::new(cx, top_y), Pos2::new(cx, bot_y)], stroke);
        painter.line_segment(
            [Pos2::new(cx - barb, top_y + barb), Pos2::new(cx, top_y)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(cx + barb, top_y + barb), Pos2::new(cx, top_y)],
            stroke,
        );
    }

    pub fn draw_tray_arrow_down(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let left_x = rect.min.x + rect.width() * 0.2;
        let right_x = rect.max.x - rect.width() * 0.2;
        let tray_y = rect.max.y - rect.height() * 0.22;
        let lip_y = tray_y - rect.height() * 0.18;
        // Tray
        painter.line_segment(
            [Pos2::new(left_x, lip_y), Pos2::new(left_x, tray_y)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(left_x, tray_y), Pos2::new(right_x, tray_y)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(right_x, tray_y), Pos2::new(right_x, lip_y)],
            stroke,
        );

        // Arrow
        let cx = rect.center().x;
        let arr_top = rect.min.y + rect.height() * 0.18;
        let arr_bot = tray_y - 2.0;
        let barb = rect.width() * 0.22;
        painter.line_segment([Pos2::new(cx, arr_top), Pos2::new(cx, arr_bot)], stroke);
        painter.line_segment(
            [Pos2::new(cx - barb, arr_bot - barb), Pos2::new(cx, arr_bot)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(cx + barb, arr_bot - barb), Pos2::new(cx, arr_bot)],
            stroke,
        );
    }

    pub fn draw_external_link(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let min_x = rect.min.x + rect.width() * 0.2;
        let max_x = rect.max.x - rect.width() * 0.2;
        let min_y = rect.min.y + rect.height() * 0.2;
        let max_y = rect.max.y - rect.height() * 0.2;
        let corner_gap = (max_x - min_x) * 0.45;

        // Box lines with top-right opening
        painter.line_segment(
            [Pos2::new(min_x, min_y), Pos2::new(min_x + corner_gap, min_y)],
            stroke,
        );
        painter.line_segment([Pos2::new(min_x, min_y), Pos2::new(min_x, max_y)], stroke);
        painter.line_segment([Pos2::new(min_x, max_y), Pos2::new(max_x, max_y)], stroke);
        painter.line_segment(
            [Pos2::new(max_x, max_y), Pos2::new(max_x, min_y + corner_gap)],
            stroke,
        );

        // Arrow pointing top-right
        let arr_start = Pos2::new(
            min_x + (max_x - min_x) * 0.35,
            max_y - (max_y - min_y) * 0.35,
        );
        let arr_end = Pos2::new(max_x, min_y);
        painter.line_segment([arr_start, arr_end], stroke);
        let barb = (max_x - min_x) * 0.35;
        painter.line_segment([Pos2::new(max_x - barb, min_y), arr_end], stroke);
        painter.line_segment([Pos2::new(max_x, min_y + barb), arr_end], stroke);
    }

    pub fn draw_stash_box(painter: &Painter, rect: Rect, color: Color32) {
        let stroke = Stroke::new(1.3_f32, color);
        let inner = rect.shrink(rect.width() * 0.15);
        painter.rect_stroke(inner, 2.0, stroke);
        let lid_y = inner.min.y + inner.height() * 0.32;
        painter.line_segment(
            [Pos2::new(inner.min.x, lid_y), Pos2::new(inner.max.x, lid_y)],
            stroke,
        );
        let handle_w = inner.width() * 0.35;
        let handle_cx = inner.center().x;
        let handle_y = lid_y + inner.height() * 0.3;
        painter.line_segment(
            [
                Pos2::new(handle_cx - handle_w * 0.5, handle_y),
                Pos2::new(handle_cx + handle_w * 0.5, handle_y),
            ],
            stroke,
        );
    }
}
