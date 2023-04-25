import json
import os.path
import random
from datetime import datetime
from glob import glob

from manim import *

sys.path.append(".")

from config import (
    BACKGROUND_COLOR,
    CLASS,
    CONTEST_END,
    CONTEST_START,
    FACE_DIR,
    MAX_SCORE,
    MEDAL,
    MEDAL_COLORS,
    MEDAL_DELAY,
    NUM_USERS,
    PATH_CUP,
    PATH_LOGO,
    PATH_MEDAL,
    PATH_NO_FACE,
    PATH_OUTPUT,
    SCREEN_DIR,
    TIMELAPSE_DURATION,
    WINNER_CONFETTI,
)

with open(os.path.join(PATH_OUTPUT, "history.json")) as f:
    history = json.load(f)

with open(os.path.join(PATH_OUTPUT, "ranking.json")) as f:
    ranking = json.load(f)


class Slide(Scene):
    def construct(self):
        self.camera.background_color = BACKGROUND_COLOR
        self.timelapse_dur = TIMELAPSE_DURATION
        self.start_time = datetime.strptime(CONTEST_START, "%Y-%m-%dT%H:%M:%S")
        self.end_time = datetime.strptime(CONTEST_END, "%Y-%m-%dT%H:%M:%S")
        self.timelapse = False

        logo = ImageMobject(PATH_LOGO)
        logo.scale(0.5)
        logo.to_corner(UP + RIGHT)
        self.add(logo)

        if NUM_USERS is not None:
            num_users = NUM_USERS
        else:
            num_users = len(ranking)
        users = list(reversed(ranking[:num_users]))

        for user in users:
            username = user["username"]
            if not os.path.exists(os.path.join(SCREEN_DIR, username)):
                raise RuntimeError(f"{username} does not have the screenshot dir")

        for user in users:
            # skip other medals
            if user["medal"] != MEDAL:
                continue

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
            province = " (%s)" % user["province"] if user["province"] else ""
            sub = Tex(f"Classe {klass}, {school}, {city}{province}")
            sub.scale(0.8)
            sub.next_to(name, DOWN)
            sub.set_x(name.get_x() - name.get_width() / 2, LEFT)
            sub.set_y(name.get_y() - 0.6)

            # face
            face_path = os.path.join(FACE_DIR, username + ".jpg")
            if os.path.exists(face_path):
                img = ImageMobject(face_path)
                img.set_height(5)
                # img.set_width(5)
                img.to_corner(DOWN + LEFT)
            else:
                print(f"!!! Face of {username} at {face_path} not found")
                img = ImageMobject(PATH_NO_FACE)

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
                if when < self.start_time or when > self.end_time:
                    continue
                self.screenshots.append((when, screen))
            self.cur_screen = 0
            self.cur_score = 0
            self.cur_time = 0.0
            self.screen = self.get_screen(0)
            self.score = self.get_score(0)
            self.play(FadeIn(self.screen), FadeIn(self.score), run_time=0.3)

            # progress bar
            self.progress = Rectangle(
                height=0.05,
                width=0.001,
                fill_color=WHITE,
                fill_opacity=1,
                stroke_width=0,
            )
            self.progress.set_x(self.screen.get_x() - self.screen.get_width() / 2)
            self.progress.set_y(
                self.screen.get_y() - self.screen.get_height() / 2 - 0.05
            )
            self.add(self.progress)
            self.cur_time = 0.0

            # render the timestamp
            self.timelapse = True
            self.always_update_mobjects = True
            self.wait(self.timelapse_dur)
            self.timelapse = False
            self.always_update_mobjects = False

            # make sure the final score is shown
            self.remove(self.score)
            self.score = self.get_score(-1)
            self.add(self.score)

            # medal
            medal = ImageMobject(PATH_MEDAL)
            medal.scale(0.75)
            medal.set_color(MEDAL_COLORS[user["medal"]])
            medal.move_to([2, 0, 0])  # TODO: fix medal Y position
            position = Tex(r"\textbf{%d}" % self.position)
            position.scale(2)
            position.move_to(medal)
            position.set_color(BLACK)

            # confetti boom
            confetti_anim = get_confetti_boom_animations(
                position.get_x(), position.get_y(), 50
            )

            self.play(
                ApplyMethod(self.score.to_corner, DOWN + RIGHT),
                FadeOut(self.screen),
                FadeOut(self.progress),
                LaggedStart(
                    AnimationGroup(
                        *confetti_anim,
                        FadeIn(medal, scale=2),
                        FadeIn(position, scale=2),
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
                    ShowCreation(po),
                    Write(po_text),
                )

            # winner confetti
            if WINNER_CONFETTI and self.position == 1:
                self.wait(1)
                confetti_anim = get_confetti_animations(150)
                confetti_spirils = list(map(turn_animation_into_updater, confetti_anim))
                self.add(*confetti_spirils)
                self.wait(15)
            else:
                self.wait(MEDAL_DELAY[user["medal"]])

            # Fade out
            fade_outs = [
                FadeOut(name),
                FadeOut(sub),
                FadeOut(img),
                FadeOut(self.score),
                FadeOut(medal),
                FadeOut(position),
            ]
            if user["po"]:
                fade_outs += [
                    FadeOut(po),
                    FadeOut(po_text),
                ]
            self.play(*fade_outs)
        self.play(FadeOut(logo))

    def get_screen(self, index):
        screen = ImageMobject(self.screenshots[index][1])
        screen.set_height(3)
        screen.to_corner(DOWN + RIGHT)
        return screen

    def get_score(self, index):
        points = int(self.history[index]["score"])
        score = Tex(f"{points} / {MAX_SCORE}")
        score.scale(1.2)
        score.next_to(self.screen, UP + RIGHT)
        score.set_x(self.screen.get_x() + self.screen.get_width() / 2, RIGHT)
        return score

    def find_cur_screen(self, now):
        changed = False
        for i in range(self.cur_screen, len(self.screenshots)):
            time, path = self.screenshots[i]
            if time >= now:
                if i != self.cur_screen:
                    changed = True
                self.cur_screen = i
                break
        return changed

    def find_cur_score(self, now: datetime):
        changed = False
        for i in range(self.cur_score, len(self.history)):
            time = self.history[i]["time"]
            if time >= now.timestamp():
                if i != self.cur_score:
                    changed = True
                self.cur_score = i
                break
        return changed

    def update_mobjects(self, dt):
        super().update_mobjects(dt)
        if self.timelapse:
            self.cur_time += dt
            now_perc = self.cur_time / self.timelapse_dur
            now_time = self.start_time + (self.end_time - self.start_time) * now_perc
            self.progress.set_width(
                self.screen.get_width() * max(0.0001, min(now_perc, 1)),
                #stretch=True, #TODO: figure this out!!
                #about_edge=LEFT,
            )
            self.progress.set_x(
                self.screen.get_x()
                - self.screen.get_width() / 2
                + self.progress.get_width() / 2
            )
            self.progress.set_y(
                self.screen.get_y() - self.screen.get_height() / 2 - 0.05
            )
            # search the current screenshot
            if self.find_cur_screen(now_time):
                self.remove(self.screen)
                screen = self.get_screen(self.cur_screen)
                self.add(screen)
                self.screen = screen
            if self.find_cur_score(now_time):
                score = self.get_score(self.cur_score)
                self.remove(self.score)
                self.add(score)
                self.score = score

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


class ConfettiSpiril(Animation):
    def __init__(self, mobject, **kwargs):
        d = {
            "x_start": 0,
            "spiril_radius": 0.5,
            "num_spirils": 4,
            "run_time": 15,
            "rate_func": None,
        }
        d.update(kwargs)
        for k,v in d.items():
            setattr(self, k, v)
        Animation.__init__(
            self,
            mobject,
            **d
        )
        mobject.next_to(self.x_start * RIGHT + FRAME_Y_RADIUS * UP, UP)
        self.total_vert_shift = FRAME_HEIGHT + mobject.get_height() + 2 * MED_SMALL_BUFF

        Animation.__init__(self, mobject, **kwargs)

    def interpolate_submobject(self, submobject, starting_submobject, alpha):
        submobject.points = np.array(starting_submobject.points)

    def interpolate_mobject(self, alpha):
        Animation.interpolate_mobject(self, alpha)
        angle = alpha * self.num_spirils * 2 * np.pi
        vert_shift = alpha * self.total_vert_shift

        start_center = self.mobject.get_center()
        self.mobject.shift(self.spiril_radius * OUT)
        self.mobject.rotate(angle, axis=UP, about_point=start_center)
        self.mobject.shift(vert_shift * DOWN)
        if alpha > 0.85:
            self.mobject.fade(1 - (1 - alpha) / (1 - 0.85))


def get_confetti_animations(num_confetti_squares):
    colors = [RED, YELLOW, GREEN, BLUE, PURPLE, RED]
    confetti_squares = [
        Square(
            side_length=0.2,
            stroke_width=0,
            fill_opacity=0.75,
            fill_color=random.choice(colors),
        )
        for x in range(num_confetti_squares)
    ]
    confetti_spirils = [
        ConfettiSpiril(
            square,
            x_start=2 * random.random() * FRAME_X_RADIUS - FRAME_X_RADIUS,
            rate_func=squish_rate_func(lambda t: t, a, a + 0.5),
        )
        for a, square in zip(np.linspace(0, 1, num_confetti_squares), confetti_squares)
    ]
    return confetti_spirils


class ConfettiBoom(Animation):
    def __init__(self, mobject, **kwargs):
        d = {
            "x_start": 0,
            "y_start": 0,
            "direction": 0,
            "speed": 2,
            "spiril_radius": 0.5,
            "num_spirils": 2,
            "run_time": 1.5,
        }
        d.update(kwargs)
        for k,v in d.items():
            setattr(self, k, v)
        Animation.__init__(
            self,
            mobject,
            **d
        )
        self.direction_v = np.array([np.cos(self.direction), np.sin(self.direction), 0])
        self.phase = random.random() * 2 * np.pi
        mobject.shift(np.array([self.x_start, self.y_start, 0]))

        Animation.__init__(self, mobject, **kwargs)

    def interpolate_submobject(self, submobject, starting_submobject, alpha):
        submobject.points = np.array(starting_submobject.points)

    def interpolate_mobject(self, alpha):
        Animation.interpolate_mobject(self, alpha)

        angle = alpha * self.num_spirils * 2 * np.pi + self.phase
        dist = alpha * self.speed

        start_center = self.mobject.get_center()
        self.mobject.rotate(angle, axis=UP, about_point=start_center)
        self.mobject.rotate(angle, axis=OUT, about_point=start_center)
        self.mobject.shift(dist * self.direction_v)
        if alpha > 0.85:
            self.mobject.fade(1 - (1 - alpha) / (1 - 0.85))


def get_confetti_boom_animations(x_start, y_start, num_confetti_squares):
    colors = [RED, YELLOW, GREEN, BLUE, PURPLE, RED]
    confetti_squares = [
        Square(
            side_length=0.2,
            stroke_width=0,
            fill_opacity=0.75,
            fill_color=random.choice(colors),
        )
        for x in range(num_confetti_squares)
    ]
    confetti_spirils = [
        ConfettiBoom(
            square,
            x_start=x_start,
            y_start=y_start,
            direction=random.random() * 2 * np.pi,
            speed=random.random() * 3,
            rate_func=lambda t: rush_from(t, 10),
        )
        for square in confetti_squares
    ]
    return confetti_spirils
