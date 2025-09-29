import sys
import json
import os.path
from datetime import datetime
from glob import glob

from manim import *

WIDTH, HEIGHT = config.frame_x_radius, config.frame_y_radius
WIDTH -= 0.5
HEIGHT -= 0.5

sys.path.append(".")

from config import *
from confetti import *

with open(os.path.join(OUTPUT_DIR, "history.json")) as f:
    history = json.load(f)

with open(os.path.join(OUTPUT_DIR, "ranking.json")) as f:
    ranking = json.load(f)

MEDAL = sys.argv[4].lower()
ranking = [u for u in reversed(ranking) if u["medal"].lower() in MEDAL_NAMES[MEDAL]][-MAX_USERS:]

print('Rendering ranking of %d students with medal "%s"...\n' % (len(ranking), MEDAL))


class Medal(Scene):
    def construct(self):
        self.camera.background_color = BACKGROUND_COLOR
        self.timelapse_dur = TIMELAPSE_DURATION
        self.start_time = [datetime.strptime(s, "%Y-%m-%dT%H:%M:%S") for s in CONTEST_START]
        self.end_time = [datetime.strptime(e, "%Y-%m-%dT%H:%M:%S") for e in CONTEST_END]
        self.timelapse = False

        logo = ImageMobject(PATH_LOGO)
        logo.scale(0.5)
        logo.to_corner(UP + RIGHT)
        self.add(logo)

        for user in ranking:
            username = user["username"]
            self.position = user["position"]
            print(f"====== Processing {username} ({self.position}) ========")

            self.screen_dir = os.path.join(SCREEN_DIR, username)
            self.history = [{"time": 0, "score": 0.0}, *history[username]]

            # cup
            if self.position <= 3:
                self.wait(2)
                self.cup(self.position)

            # name
            name = Tex(user["name"])
            name.scale(1.4)
            name.to_corner(UP + LEFT)

            # school
            klass = CLASS[user["class"]]
            school = user["school"]
            city = user["city"]
            province = " (%s)" % user["province"] if user.get("province") else ""
            sub = Tex(f"Classe {klass}, {school}, {city}{province}")
            sub.scale(0.8)
            sub.next_to(name, DOWN)
            sub.set_x(name.get_x() - name.width / 2, LEFT)
            sub.set_y(name.get_y() - 0.6)

            # face
            face_path = os.path.join(FACE_DIR, username + ".jpg")
            if os.path.exists(face_path):
                img = ImageMobject(face_path)
            else:
                print(f"!!! Face of {username} at {face_path} not found")
                img = ImageMobject(PATH_NO_FACE)
            img.height = 5
            img.to_corner(DOWN + LEFT)

            # fade in the name and the picture
            self.wait(1)
            write_name = Write(name)
            self.play(
                write_name,
                Write(sub, run_time=write_name.run_time),
                FadeIn(img, shift=RIGHT),
            )

            # timelapse
            self.screenshots = []
            for screen in sorted(glob(os.path.join(self.screen_dir, "*"))):
                when = os.path.basename(screen)[: -len(".png")]
                try:
                    when = datetime.strptime(when, "%Y-%m-%dT%H:%M:%S.%f")
                except ValueError:
                    continue
                for s,e in zip(self.start_time, self.end_time):
                    if when >= s and when <= e:
                        self.screenshots.append((when, screen))
                        break
            if len(self.screenshots) == 0:
                print(f"!!! Screenshots of {username} at {self.screen_dir} not found")
            self.cur_screen = 0
            self.cur_score = 0
            self.cur_time = 0.0
            self.cur_datetime = self.start_time[0]

            self.screen = self.get_screen(0)
            self.score = self.get_score(0)

            self.play(FadeIn(self.screen), FadeIn(self.score), run_time=0.3)

            def screen_updater(screen):
                if self.find_cur_screen(self.cur_datetime):
                    screen.become(self.get_screen(self.cur_screen))

            self.screen.add_updater(screen_updater)

            def score_updater(score):
                if self.find_cur_score(self.cur_datetime):
                    score.become(self.get_score(self.cur_score))

            self.score.add_updater(score_updater)

            # progress bar
            self.progress = Rectangle(
                height=0.05,
                width=0.0005,
                fill_color=WHITE,
                fill_opacity=1,
                stroke_width=0,
            )
            self.progress.set_x(self.screen.get_x() - self.screen.width / 2)
            self.progress.set_y(self.screen.get_y() - self.screen.height / 2 - 0.05)

            def progressbar_updater(bar):
                now_perc = self.cur_time / self.timelapse_dur
                bar.stretch_to_fit_width(
                    self.screen.width * max(0.0001, min(now_perc, 1))
                )
                bar.set_x(self.screen.get_x() - self.screen.width / 2 + bar.width / 2)
                bar.set_y(self.screen.get_y() - self.screen.height / 2 - 0.05)

            self.progress.add_updater(progressbar_updater)
            self.add(self.progress)
            self.cur_time = 0.0

            # render the timestamp
            self.timelapse = True
            self.always_update_mobjects = True
            self.wait(self.timelapse_dur)
            self.timelapse = False
            self.always_update_mobjects = False

            # make sure the final score is shown
            self.score.become(self.get_score(-1))
            self.screen.become(self.get_screen(-1))

            # medal
            medal = ImageMobject(PATH_MEDAL)
            medal.scale(0.75)
            medal.set_color(MEDAL_COLORS[user["medal"]])
            medal.move_to([2, 0, 0])  # TODO: fix medal Y position

            # confetti boom
            confetti_anim = get_confetti_boom_animations(
                medal.get_x(), medal.get_y(), 50
            )
            position_anim = [FadeIn(medal, scale=2)]
            if self.position != float("inf"):
                position = Tex(r"\textbf{%d}" % self.position)
                position.scale(2)
                position.move_to(medal)
                position.set_color(BLACK)
                position_anim.append(FadeIn(position, scale=2))

            self.play(
                ApplyMethod(self.score.to_corner, DOWN + RIGHT),
                FadeOut(self.screen),
                FadeOut(self.progress),
                LaggedStart(
                    AnimationGroup(
                        *confetti_anim,
                        *position_anim,
                    )
                ),
            )

            # po
            po = Circle(stroke_color=WHITE)
            po.scale(0.5)
            po.next_to(medal)
            po_text = Tex("PO")
            po_text.move_to(po)
            if user["po"]:
                self.play(
                    Create(po),
                    Write(po_text),
                )

            # winner confetti
            confetti = []
            if WINNER_CONFETTI and self.position == 1:
                # self.wait(1)
                confetti = get_confetti_animations(150)
                self.add(*confetti)
                self.wait(WINNER_CONFETTI_DURATION)
            else:
                self.wait(MEDAL_DELAY[user["medal"]])

            # Fade out
            fade_outs = [
                FadeOut(name),
                FadeOut(sub),
                FadeOut(img),
                FadeOut(self.score),
                FadeOut(medal),
            ]
            if self.position != float("inf"):
                fade_outs.append(FadeOut(position))
            fade_outs += [FadeOut(c) for c in confetti]
            if user["po"]:
                fade_outs += [
                    FadeOut(po),
                    FadeOut(po_text),
                ]
            self.play(*fade_outs)
        self.play(FadeOut(logo))

    def get_screen(self, index):
        screen = ImageMobject(self.screenshots[index][1] if len(self.screenshots) else PATH_NO_SCREEN)
        screen.height = 3
        screen.to_corner(DOWN + RIGHT)
        return screen

    def get_score(self, index):
        points = int(self.history[index]["score"])
        score = Tex(f"{points} / {MAX_SCORE}")
        score.scale(1.2)
        score.next_to(self.screen, UP + RIGHT)
        score.set_x(self.screen.get_x() + self.screen.width / 2, RIGHT)
        return score

    def find_cur_screen(self, now):
        changed = False
        while self.cur_screen+1 < len(self.screenshots):
            if self.screenshots[self.cur_screen+1][0] <= now.timestamp():
                self.cur_screen += 1
                changed = True
            else:
                break
        return changed

    def find_cur_score(self, now: datetime):
        changed = False
        while self.cur_score+1 < len(self.history):
            if self.history[self.cur_score+1]["time"] <= now.timestamp():
                self.cur_score += 1
                changed = True
            else:
                break
        return changed

    def update_mobjects(self, dt):
        super().update_mobjects(dt)
        if self.timelapse:
            self.cur_time += dt
            now_perc = self.cur_time / self.timelapse_dur
            i = min(int(now_perc * len(self.start_time)), len(self.start_time)-1)
            now_perc -= i / len(self.start_time)
            self.cur_datetime = (
                self.start_time[i] + (self.end_time[i] - self.start_time[i]) * now_perc
            )

    def cup(self, pos_num):
        cup = SVGMobject(PATH_CUP)
        cup.scale(2)
        cup.set_color(MEDAL_COLORS["gold"])

        pos = Tex(str(pos_num), background_stroke_width=0)
        pos.set_color(BACKGROUND_COLOR)
        pos.scale(3)
        pos.shift(1.3 * UP)

        self.play(Write(cup, run_time=2), Write(pos, run_time=0.001))
        self.wait(2)
        self.play(FadeOut(cup, shift=DOWN), FadeOut(pos, shift=DOWN))


class Gold(Medal):
    pass

class Silver(Medal):
    pass

class Bronze(Medal):
    pass

class Mention(Scene):
    def construct(self):
        self.camera.background_color = BACKGROUND_COLOR

        ARRX, ARRY = MENTION_ARRAY
        BUNCH = ARRX * (ARRY-1)
        fade_list = []
        for i,user in enumerate(ranking):
            username = user["username"]
            print(f"====== Processing {username} ========")
            x = i % ARRX
            y = (i//ARRX) % ARRY

            scale = 0.7

            # face
            face_path = os.path.join(FACE_DIR, username + ".jpg")
            if os.path.exists(face_path):
                img = ImageMobject(face_path)
            else:
                print(f"!!! Face of {username} at {face_path} not found")
                img = ImageMobject(PATH_NO_FACE)
            img.height = 1.5*scale
            img.to_corner(DOWN + LEFT)
            img.set_x(-WIDTH + 2*WIDTH*x/ARRX, LEFT)
            img.set_y(HEIGHT - 2*HEIGHT*y/ARRY, UP)

            # name
            name = Tex(user["name"])
            name.scale(1.4*scale)
            name.set_x(img.get_x(RIGHT) + 0.2, LEFT)
            name.set_y(img.get_y(UP), UP)

            # school
            klass = CLASS[user["class"]]
            school = user["school"]
            city = user["city"]
            province = " (%s)" % user["province"] if user.get("province") else ""

            subsub = Tex(f"{city}{province}")
            subsub.scale(0.8*scale)
            subsub.set_x(name.get_x(LEFT), LEFT)
            subsub.set_y(img.get_y(DOWN), DOWN)

            sub = Tex(f"Classe {klass}, {school}")
            sub.scale(0.8*scale)
            sub.set_x(name.get_x(LEFT), LEFT)
            sub.set_y((name.get_y(DOWN) + subsub.get_y(UP))/2)

            # po
            po = Circle(stroke_color=WHITE)
            po.scale(0.5)
            po.next_to(name)
            po_text = Tex("PO")
            po_text.move_to(po)
            if user["po"]:
                self.play(
                    Create(po),
                    Write(po_text),
                )

            # Fade out
            fade_list.append([
                FadeOut(name),
                FadeOut(sub),
                FadeOut(subsub),
                FadeOut(img),
            ] + ([FadeOut(po)] if user["po"] else []))
            fade_outs = []
            if len(fade_list) >= BUNCH:
                fade_outs = fade_list[0]
                fade_list = fade_list[1:]

            # fade in the name and the picture
            self.wait(1)
            write_name = Write(name)
            self.play(
                write_name,
                Write(sub, run_time=write_name.run_time),
                Write(subsub, run_time=write_name.run_time),
                FadeIn(img, shift=RIGHT),
                *fade_outs
            )
        while fade_list:
            fade_outs = fade_list[0]
            fade_list = fade_list[1:]
            # fade in the name and the picture
            self.wait(1)
            self.play(*fade_outs)
        self.wait(1)
