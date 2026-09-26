//! 16:9 videos (port of ranking.py)

use std::collections::VecDeque;
use std::sync::Arc;

use super::{BadgeLayout, Ctx, Timelapse, badge, confetti_boom, confetti_rain, cup, po_mark, screen_frames};
use crate::config::{Color, Group, Medal};
use crate::data::User;
use crate::timeline::{Anim, BUFF, DL, DR, EDGE_BUFF, Mob, RIGHT, Scene, UL, UR};

/// Frame height in units, as in manim
pub const FRAME_H: f64 = 8.0;

pub fn build(ctx: &Ctx, fps: f64, aspect: f64) -> Scene {
    let mut scene = Scene::new(fps, FRAME_H * aspect, FRAME_H, Some(ctx.config.colors.background));
    if ctx.medal == Medal::Mention {
        mention(&mut scene, ctx);
    } else {
        medal(&mut scene, ctx);
    }
    scene
}

/// Card i of a mx x my grid over the whole frame
fn badge_at(scene: &mut Scene, ctx: &Ctx, i: usize, user: &User, g: Group) -> (Vec<Anim>, Vec<Anim>) {
    let (mx, my, scale) = (g.cols, g.rows, g.scale);
    let width = scene.fw / 2.0 - 0.5;
    let x = -width + 2.0 * width * (i % mx) as f64 / mx as f64;
    // rows are packed together, and the whole grid is centered vertically
    let row_h = 1.5 * scale + ctx.config.groups.row_gap;
    let y = my as f64 * row_h / 2.0 - row_h * ((i / mx) % my) as f64;
    let layout = BadgeLayout {
        x,
        y,
        center_y: false,
        scale,
        cell_w: 2.0 * width / mx as f64,
        medal_room: 0.95 * scale + 0.25,
        po_room: 0.8 * scale + 0.25,
    };
    badge(scene, ctx, user, layout)
}

fn mention(scene: &mut Scene, ctx: &Ctx) {
    let Some(&g) = ctx.config.groups.mention.first() else { return };
    // a card fades out when its row is needed again
    let bunch = g.cols * (g.rows - 1);
    let delay = ctx.delay();
    let mut shown: VecDeque<Vec<Anim>> = VecDeque::new();
    for (i, user) in ctx.users.iter().enumerate() {
        println!("====== Processing {} ========", user.username);
        let (mut play, fade) = badge_at(scene, ctx, i, user, g);
        shown.push_back(fade);
        if shown.len() >= bunch.max(1) {
            play.extend(shown.pop_front().unwrap());
        }
        scene.wait(delay);
        scene.play(play);
    }
    while let Some(fade) = shown.pop_front() {
        scene.wait(delay);
        scene.play(fade);
    }
    scene.wait(1.0);
}

fn medal(scene: &mut Scene, ctx: &Ctx) {
    let config = ctx.config;
    let frame = scene.frame_size();
    let fonts = ctx.fonts;
    let mut groups: VecDeque<Group> = ctx.groups().into();
    let (mut count, mut group_count) = (0, 0);
    let (mut group_play, mut group_fade) = (vec![], vec![]);

    // logo.png is 1200px tall: 0.2 makes it as big as the old 600px logo at 0.5, minus 20%
    let logo_mob = ctx.logo().scale(0.2).to_edge(UR, frame, EDGE_BUFF);
    let mut logo = scene.obj(logo_mob.clone());
    scene.add(&[logo]);

    for user in &ctx.users {
        println!(
            "====== Processing {} ({}) ========",
            user.username,
            user.position.map_or("-".into(), |p| p.to_string())
        );
        let mut rng = super::Rng::new(&user.username);

        if let Some(&g) = groups.front() {
            let (p, f) = badge_at(scene, ctx, count, user, g);
            count += 1;
            group_play.extend(p);
            group_fade.extend(f);
        }

        // cup
        if let Some(p @ 1..=3) = user.position {
            scene.wait(2.0);
            cup(scene, ctx, p);
        }

        // name, school, face
        let name = Mob::shape(fonts.tex(&user.name, 1.4)).to_edge(UL, frame, EDGE_BUFF);
        let city = format!("{}, {}", user.class_school(config), user.city_province());
        let sub = Mob::shape(fonts.tex(&city, 0.8)).set_left(name.left()).set_y(name.y - 0.6);
        let face = ctx.face(user).set_height(5.0).to_edge(DL, frame, EDGE_BUFF);
        let (name, sub, face) = (scene.obj(name), scene.obj(sub), scene.obj(face));

        scene.wait(1.0);
        let rt = scene.run_time(&Anim::write(name));
        scene.play(vec![Anim::write(name), Anim::write(sub).run_time(rt), Anim::fade_in(face).shift(RIGHT)]);

        // timelapse
        let screenshots = ctx.screenshots(user);
        let history = ctx.data.history_of(&user.username);
        let place_screen = |m: Mob| m.set_height(3.0).to_edge(DR, frame, EDGE_BUFF);
        let score_mob = |points: f64, screen: &Mob| {
            Mob::shape(fonts.tex(&ctx.score_text(points), 1.2)).next_to(screen, UR, BUFF).set_right(screen.right())
        };
        let first_screen = if screenshots.is_empty() {
            place_screen(Mob::image(ctx.no_screen()))
        } else {
            place_screen(Mob::image(ctx.images.file_or(&screenshots[0].1, None, || ctx.no_screen())))
        };
        let screen = scene.obj(first_screen.clone());
        let score = scene.obj(score_mob(0.0, &first_screen));
        scene.play(vec![Anim::fade_in(screen).run_time(0.3), Anim::fade_in(score).run_time(0.3)]);

        let tl = Timelapse::new(scene, config.timing.timelapse_duration);
        let (screens, last_screen) = screen_frames(ctx, &tl, &screenshots, place_screen);
        let screens = Arc::new(screens);
        let times: Vec<f64> = history.iter().map(|c| c.time).collect();
        let score_idx = tl.indices(&ctx.contest, &times);
        // one text per distinct score
        let mut score_frames = vec![];
        let mut cache: std::collections::HashMap<(usize, *const Mob), Arc<Mob>> = Default::default();
        for (k, &i) in score_idx.iter().enumerate() {
            let s = &screens[k];
            score_frames.push(
                cache.entry((i, Arc::as_ptr(s))).or_insert_with(|| Arc::new(score_mob(history[i].score, s))).clone(),
            );
        }
        let final_score = score_mob(history.last().map_or(0.0, |c| c.score), &last_screen);
        {
            let screens = screens.clone();
            scene.become_func(screen, Arc::new(move |t| Some((*screens[tl.frame(t)]).clone())));
            scene.become_func(score, Arc::new(move |t| Some((*score_frames[tl.frame(t)]).clone())));
        }

        // progress bar under the screen
        let progress = {
            let screens = screens.clone();
            scene.func(Arc::new(move |t| {
                let k = tl.frame(t);
                let s = &screens[k];
                let fraction = (tl.elapsed(k) / tl.duration).clamp(0.0001, 1.0);
                let w = s.width() * fraction;
                Some(Mob::rect(w, 0.05, Color::WHITE).set_left(s.left()).set_y(s.bottom() - 0.05))
            }))
        };
        scene.add(&[progress]);
        scene.wait(tl.duration);

        // the final score and screenshot
        scene.become_mob(score, final_score.clone());
        scene.become_mob(screen, (*last_screen).clone());

        // medal
        let medal = ctx.medal_img().scale(0.75).move_to(2.0, 0.0);
        let mut anims = vec![];
        let corner = final_score.clone().to_edge(DR, frame, EDGE_BUFF);
        anims.push(Anim::shift_by(score, corner.x - final_score.x, corner.y - final_score.y));
        anims.push(Anim::fade_out(screen));
        anims.push(Anim::fade_out(progress));
        anims.extend(confetti_boom(scene, &mut rng, medal.x, medal.y, 50));
        let medal_id = scene.obj(medal.clone());
        anims.push(Anim::fade_in(medal_id).scale(2.0));
        let position = user.position.map(|p| {
            let pos = Mob::shape(fonts.tex_bold(&p.to_string(), 2.0, Color::BLACK)).move_to(medal.x, medal.y);
            scene.obj(pos)
        });
        if let Some(pos) = position {
            anims.push(Anim::fade_in(pos).scale(2.0));
        }
        scene.play(anims);

        // po
        let mut fade_outs = vec![name, sub, face, score, medal_id];
        if user.po {
            let (circle, text) = po_mark(fonts, &medal, 0.5, 1.0);
            let (circle, text) = (scene.obj(circle), scene.obj(text));
            scene.play(vec![Anim::create(circle), Anim::write(text)]);
            fade_outs.extend([circle, text]);
        }

        // winner confetti
        let winner = config.timing.winner_confetti_duration;
        if winner > 0.0 && user.position == Some(1) {
            fade_outs.extend(confetti_rain(scene, &mut rng, 150, winner));
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
            scene.play(vec![Anim::fade_out(logo)]);
            scene.play(std::mem::take(&mut group_play));
            scene.wait(2.0);
            scene.play(std::mem::take(&mut group_fade));
            logo = scene.obj(logo_mob.clone());
            scene.play(vec![Anim::fade_in(logo)]);
            count = 0;
            groups.pop_front();
        }
    }
    scene.play(vec![Anim::fade_out(logo)]);
}
