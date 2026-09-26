//! 64:9 videos (port of ranking_wide.py)

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::{
    BadgeLayout, Ctx, Timelapse, badge, confetti_boom, confetti_rain, cup, po_mark, screen_frames, smoothed, tween,
};
use crate::config::{Color, Group, Medal};
use crate::data::User;
use crate::timeline::{Anim, Mob, RIGHT, Scene};

pub const FRAME_H: f64 = 8.0;

/// Layout of the wide frame, from config.toml [wide]
struct Layout {
    fw: f64,
    fh: f64,
    /// zone -> (center x, width)
    zones: HashMap<&'static str, (f64, f64)>,
    content_top: f64,
    content_bottom: f64,
    content_cy: f64,
    /// Horizontal extent between the two logo zones
    center_x: (f64, f64),
    /// The medal scenes of the 16:9 version show the medal at 0.75; the PO circle and the
    /// position scale with the medal relative to that
    medal_k: f64,
    /// Score from which the top bar takes the color of each medal
    thresholds: Vec<(f64, Color)>,
}

impl Layout {
    fn new(ctx: &Ctx, fw: f64) -> Layout {
        let w = &ctx.config.wide;
        let fh = FRAME_H;
        let mut zones = HashMap::new();
        let mut left = -fw / 2.0;
        let z = &w.zone_fractions;
        for (name, fraction) in [
            ("logo_left", z.logo_left),
            ("info", z.info),
            ("score", z.score),
            ("screen", z.screen),
            ("logo_right", z.logo_right),
        ] {
            zones.insert(name, (left + fw * fraction / 2.0, fw * fraction));
            left += fw * fraction;
        }
        let content_top = fh / 2.0 - w.bar_height - w.margin;
        let content_bottom = -fh / 2.0 + w.margin;
        let (ll, lr) = (zones["logo_left"], zones["logo_right"]);
        // the lowest final score among the official contestants with each medal
        let mut thresholds = vec![];
        for m in [Medal::Gold, Medal::Silver, Medal::Bronze] {
            let min = ctx
                .data
                .ranking
                .iter()
                .filter(|u| u.position.is_some() && ctx.config.medal_of(&u.medal) == Some(m))
                .map(|u| ctx.data.final_score(&u.username))
                .reduce(f64::min);
            if let (Some(min), Some(c)) = (min, ctx.config.medal_color(m)) {
                thresholds.push((min, c));
            }
        }
        Layout {
            fw,
            fh,
            zones,
            content_top,
            content_bottom,
            content_cy: (content_top + content_bottom) / 2.0,
            center_x: (ll.0 + ll.1 / 2.0 + w.margin, lr.0 - lr.1 / 2.0 - w.margin),
            medal_k: w.medal_scale / 0.75,
            thresholds,
        }
    }

    fn content_h(&self) -> f64 {
        self.content_top - self.content_bottom
    }

    fn bar_color(&self, ctx: &Ctx, points: f64) -> Color {
        self.thresholds.iter().find(|(min, _)| points >= *min).map_or(ctx.config.wide.bar_color_no_medal, |(_, c)| *c)
    }

    fn bar(&self, ctx: &Ctx, points: f64) -> Mob {
        let w = &ctx.config.wide;
        let fraction = (points / ctx.config.contest.max_score).clamp(0.0, 1.0);
        Mob::rect((self.fw * fraction).max(0.001), w.bar_height, self.bar_color(ctx, points))
            .set_left(-self.fw / 2.0)
            .set_top(self.fh / 2.0)
    }

    /// Logo centered in one of the two logo zones
    fn logo(&self, ctx: &Ctx, zone: &str) -> Mob {
        let w = &ctx.config.wide;
        let (cx, zw) = self.zones[zone];
        let logo = ctx.logo().set_height(self.content_h() * w.logo_scale);
        logo.fit_width((zw - 2.0 * w.margin) * w.logo_scale).move_to(cx, self.content_cy)
    }

    /// x and y ranges of the recap grid of a group: sized by its scale, centered between the logos
    fn group_area(&self, ctx: &Ctx, g: Group) -> ((f64, f64), (f64, f64)) {
        let tile_w =
            (ctx.config.wide.group_tile_width * g.scale).min((self.center_x.1 - self.center_x.0) / g.cols as f64);
        let row_h = 1.5 * g.scale + ctx.config.groups.row_gap;
        let cx = (self.center_x.0 + self.center_x.1) / 2.0;
        let (cols, rows) = (g.cols as f64, g.rows as f64);
        (
            (cx - cols * tile_w / 2.0, cx + cols * tile_w / 2.0),
            (self.content_cy - rows * row_h / 2.0, self.content_cy + rows * row_h / 2.0),
        )
    }
}

pub fn build(ctx: &Ctx, fps: f64, aspect: f64) -> Scene {
    let fw = FRAME_H * aspect;
    let mut scene = Scene::new(fps, fw, FRAME_H, Some(ctx.config.colors.background));
    let layout = Layout::new(ctx, fw);
    if ctx.medal == Medal::Mention {
        mention(&mut scene, ctx, &layout);
    } else {
        medal(&mut scene, ctx, &layout);
    }
    scene
}

/// Card i of a mx x my grid spanning x_range and y_range (the whole frame by default)
fn badge_in(
    scene: &mut Scene,
    ctx: &Ctx,
    i: usize,
    user: &User,
    g: Group,
    x_range: Option<(f64, f64)>,
    y_range: Option<(f64, f64)>,
) -> (Vec<Anim>, Vec<Anim>) {
    let (width, height) = (scene.fw / 2.0 - 0.5, scene.fh / 2.0 - 0.5);
    let (xl, xr) = x_range.unwrap_or((-width, width));
    let (yb, yt) = y_range.unwrap_or((-height, height));
    let (mx, my) = (g.cols, g.rows);
    let (cell_w, cell_h) = ((xr - xl) / mx as f64, (yt - yb) / my as f64);
    let layout = BadgeLayout {
        x: xl + cell_w * (i % mx) as f64,
        y: yt - cell_h * (((i / mx) % my) as f64 + 0.5),
        center_y: true,
        scale: g.scale,
        cell_w,
        medal_room: 1.3 * g.scale,
        po_room: 0.9 * g.scale,
    };
    badge(scene, ctx, user, layout)
}

fn mention(scene: &mut Scene, ctx: &Ctx, l: &Layout) {
    // the logos stay on the sides, the cards are tiled in between
    let logos = [scene.obj(l.logo(ctx, "logo_left")), scene.obj(l.logo(ctx, "logo_right"))];
    scene.add(&logos);
    let Some(&g) = ctx.config.groups.mention.first() else { return };
    // a card is faded out only when its slot is needed again
    let bunch = g.cols * g.rows;
    let mut shown: VecDeque<Vec<Anim>> = VecDeque::new();
    for (i, user) in ctx.users.iter().enumerate() {
        println!("====== Processing {} ========", user.username);
        let (mut play, fade) = badge_in(scene, ctx, i, user, g, Some(l.center_x), None);
        shown.push_back(fade);
        if shown.len() >= bunch.max(1) {
            play.extend(shown.pop_front().unwrap());
        }
        scene.wait(ctx.delay());
        scene.play(play);
    }
    // everything that is still on screen goes away together
    scene.wait(2.0);
    let mut last: Vec<Anim> = shown.into_iter().flatten().collect();
    last.extend(logos.map(Anim::fade_out));
    scene.play(last);
    scene.wait(1.0);
}

fn medal(scene: &mut Scene, ctx: &Ctx, l: &Layout) {
    let config = ctx.config;
    let w = &config.wide;
    let fonts = ctx.fonts;
    let mut groups: VecDeque<Group> = ctx.groups().into();
    let (mut count, mut group_count) = (0, 0);
    let (mut group_play, mut group_fade) = (vec![], vec![]);

    let logos = [scene.obj(l.logo(ctx, "logo_left")), scene.obj(l.logo(ctx, "logo_right"))];
    scene.add(&logos);

    for user in &ctx.users {
        println!(
            "====== Processing {} ({}) ========",
            user.username,
            user.position.map_or("-".into(), |p| p.to_string())
        );
        let mut rng = super::Rng::new(&user.username);

        if let Some(&g) = groups.front() {
            let (x, y) = l.group_area(ctx, g);
            let (p, f) = badge_in(scene, ctx, count, user, g, Some(x), Some(y));
            count += 1;
            group_play.extend(p);
            group_fade.extend(f);
        }

        if let Some(p @ 1..=3) = user.position {
            scene.wait(2.0);
            cup(scene, ctx, p);
        }

        // name, class + school + city and face, stacked and centered in the info zone
        let info = info(ctx, l, user);
        let info: Vec<_> = info.into_iter().map(|m| scene.obj(m)).collect();
        scene.wait(1.0);
        let rt = scene.run_time(&Anim::write(info[0]));
        scene.play(vec![Anim::write(info[0]), Anim::write(info[1]).run_time(rt), Anim::fade_in(info[2]).shift(RIGHT)]);

        // timelapse
        let screenshots = ctx.screenshots(user);
        let history = ctx.data.history_of(&user.username);
        let (scx, szw) = l.zones["screen"];
        let place_screen = |m: Mob| {
            let max_h = l.content_h() - 0.5; // room for the progress bar
            let m = m.set_width(szw - 2.0 * w.margin);
            let m = if m.height() > max_h { m.set_height(max_h) } else { m };
            m.move_to(scx, l.content_cy)
        };
        let score_mob = |points: f64| {
            Mob::shape(fonts.tex(&ctx.score_text(points), w.score_scale)).move_to(l.zones["score"].0, l.content_cy)
        };
        let first_screen = if screenshots.is_empty() {
            place_screen(Mob::image(ctx.no_screen()))
        } else {
            place_screen(Mob::image(ctx.images.file_or(&screenshots[0].1, None, || ctx.no_screen())))
        };
        let screen = scene.obj(first_screen);
        let score = scene.obj(score_mob(0.0));
        let bar = scene.obj(l.bar(ctx, 0.0));
        scene.play(vec![
            Anim::fade_in(screen).run_time(0.3),
            Anim::fade_in(score).run_time(0.3),
            Anim::fade_in(bar).run_time(0.3),
        ]);

        let tl = Timelapse::new(scene, config.timing.timelapse_duration);
        let (screens, last_screen) = screen_frames(ctx, &tl, &screenshots, place_screen);
        let screens = Arc::new(screens);
        let times: Vec<f64> = history.iter().map(|c| c.time).collect();
        let score_idx = tl.indices(&ctx.contest, &times);
        let mut texts: HashMap<usize, Arc<Mob>> = HashMap::new();
        let score_frames: Vec<Arc<Mob>> = score_idx
            .iter()
            .map(|&i| texts.entry(i).or_insert_with(|| Arc::new(score_mob(history[i].score))).clone())
            .collect();
        let targets: Vec<f64> = score_idx.iter().map(|&i| history[i].score).collect();
        let bar_values = smoothed(&tl, &targets, w.bar_speed);
        let final_points = history.last().map_or(0.0, |c| c.score);
        {
            let screens = screens.clone();
            scene.become_func(screen, Arc::new(move |t| Some((*screens[tl.frame(t)]).clone())));
            scene.become_func(score, Arc::new(move |t| Some((*score_frames[tl.frame(t)]).clone())));
        }
        // the bar follows the score, then catches up with the final one during the medal animation
        let catch_up = tl.start + tl.frames as f64 / tl.fps;
        {
            let bars: Vec<Mob> = bar_values.iter().map(|&v| l.bar(ctx, v)).collect();
            let from = *bar_values.last().unwrap();
            let (thresholds, fw, fh, max, bh, no_medal) =
                (l.thresholds.clone(), l.fw, l.fh, config.contest.max_score, w.bar_height, w.bar_color_no_medal);
            scene.become_func(
                bar,
                Arc::new(move |t| {
                    if t < catch_up {
                        return Some(bars[tl.frame(t)].clone());
                    }
                    let v = tween(from, final_points, catch_up, t);
                    let color = thresholds.iter().find(|(min, _)| v >= *min).map_or(no_medal, |(_, c)| *c);
                    let fraction = (v / max).clamp(0.0, 1.0);
                    Some(Mob::rect((fw * fraction).max(0.001), bh, color).set_left(-fw / 2.0).set_top(fh / 2.0))
                }),
            );
        }

        let progress = {
            let screens = screens.clone();
            scene.func(Arc::new(move |t| {
                let k = tl.frame(t);
                let s = &screens[k];
                let fraction = (tl.elapsed(k) / tl.duration).clamp(0.0001, 1.0);
                Some(Mob::rect(s.width() * fraction, 0.05, Color::WHITE).set_left(s.left()).set_y(s.bottom() - 0.05))
            }))
        };
        scene.add(&[progress]);
        scene.wait(tl.duration);

        scene.become_mob(score, score_mob(final_points));
        scene.become_mob(screen, (*last_screen).clone());

        // medal, in the place of the screenshot
        let medal = ctx.medal_img().scale(w.medal_scale).move_to(scx, l.content_cy);
        let mut anims = vec![Anim::hold(bar, 1.0), Anim::fade_out(screen), Anim::fade_out(progress)];
        anims.extend(confetti_boom(scene, &mut rng, medal.x, medal.y, 50));
        let medal_id = scene.obj(medal.clone());
        anims.push(Anim::fade_in(medal_id).scale(2.0));
        let position = user.position.map(|p| {
            scene.obj(
                Mob::shape(fonts.tex_bold(&p.to_string(), 2.0 * l.medal_k, Color::BLACK)).move_to(medal.x, medal.y),
            )
        });
        if let Some(pos) = position {
            anims.push(Anim::fade_in(pos).scale(2.0));
        }
        scene.play(anims);
        // the bar is still until it fades out
        scene.become_mob(bar, l.bar(ctx, final_points));

        let mut fade_outs = info.clone();
        fade_outs.extend([score, bar, medal_id]);
        if user.po {
            let (circle, text) = po_mark(fonts, &medal, 0.5 * l.medal_k, l.medal_k);
            let (circle, text) = (scene.obj(circle), scene.obj(text));
            scene.play(vec![Anim::create(circle), Anim::write(text)]);
            fade_outs.extend([circle, text]);
        }

        // winner confetti (the number of squares follows the width of the frame)
        let winner = config.timing.winner_confetti_duration;
        if winner > 0.0 && user.position == Some(1) {
            let n = (150.0 * l.fw / (l.fh * 16.0 / 9.0)).round() as usize;
            fade_outs.extend(confetti_rain(scene, &mut rng, n, winner));
            scene.wait(winner);
        } else {
            scene.wait(ctx.delay());
        }
        fade_outs.extend(position);
        scene.play(fade_outs.into_iter().map(Anim::fade_out).collect());

        if let Some(&g) = groups.front()
            && count == g.size
        {
            group_count += 1;
            println!("====== Group {group_count} ({}x{}) ========", g.cols, g.rows);
            scene.play(std::mem::take(&mut group_play));
            scene.wait(2.0);
            scene.play(std::mem::take(&mut group_fade));
            count = 0;
            groups.pop_front();
        }
    }
    scene.play(logos.map(Anim::fade_out).to_vec());
}

/// Name, class + school + city, and face, stacked and centered in the info zone
fn info(ctx: &Ctx, l: &Layout, user: &User) -> Vec<Mob> {
    let w = &ctx.config.wide;
    let (cx, zw) = l.zones["info"];
    let max_w = zw - 2.0 * w.margin;
    let name = Mob::shape(ctx.fonts.tex(&user.name, w.name_scale)).fit_width(max_w);
    let text = format!("{}, {}", user.class_school(ctx.config), user.city_province());
    let line = Mob::shape(ctx.fonts.tex(&text, w.info_scale)).fit_width(max_w);
    let face = ctx.face(user);

    // the face takes the height left by the text; the whole stack is centered
    let gaps = [0.35, 0.5, 0.0];
    let text_h = name.height() + line.height() + gaps.iter().sum::<f64>();
    let face = face.set_height((l.content_h() - text_h).min(max_w));
    let mut y = l.content_cy + (text_h + face.height()) / 2.0;
    [name, line, face]
        .into_iter()
        .zip(gaps)
        .map(|(m, gap)| {
            let m = m.set_x(cx).set_top(y);
            y -= m.height() + gap;
            m
        })
        .collect()
}
