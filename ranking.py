import sys
import json
import os.path
from datetime import datetime
from glob import glob

from manim import *
import pillow_jxl  # noqa: F401 -- registers JPEG XL support in Pillow

# Manim keeps only 100 partial movie files by default and deletes the oldest ones after each
# render; a full medal scene has more animations than that, so it would lose its cache.
config.max_files_cached = 1000

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

# The scene name is found by value, so extra manim options (e.g. --media_dir) can go anywhere
MEDAL = next((a.lower() for a in sys.argv[1:] if a.lower() in MEDAL_NAMES), None)
if MEDAL is None:
    raise SystemExit("Usage: manim render [options] ranking.py <Gold|Silver|Bronze|Mention>")
ranking = [u for u in reversed(ranking) if u["medal"].lower() in MEDAL_NAMES[MEDAL]][-MAX_USERS:]

def group_size(group):
    """Number of contestants of a group: the optional 4th element, else columns * rows."""
    return group[3] if len(group) > 3 else group[0] * group[1]


print('Rendering ranking of %d students with medal "%s"...\n' % (len(ranking), MEDAL))


class Mention(Scene):
    def construct(self):
        self.camera.background_color = BACKGROUND_COLOR

        ARRX, ARRY, SCALE = GROUPS_ARRAY["mention"][0]
        BUNCH = ARRX * (ARRY-1)
        fade_list = []
        for i,user in enumerate(ranking):
            username = user["username"]
            print(f"====== Processing {username} ========")

            play_list, fade_bunch = Mention.student_badge(i, user, ARRX, ARRY, SCALE)
            fade_list.append(fade_bunch)
            fade_outs = []
            if len(fade_list) >= BUNCH:
                fade_outs = fade_list[0]
                fade_list = fade_list[1:]

            # fade in the name and the picture
            self.wait(MEDAL_DELAY["mention"])
            self.play(
                *play_list,
                *fade_outs
            )
        while fade_list:
            fade_outs = fade_list[0]
            fade_list = fade_list[1:]
            # fade in the name and the picture
            self.wait(MEDAL_DELAY["mention"])
            self.play(*fade_outs)
        self.wait(1)

    def student_badge(i, user, mx, my, scale):
            x = i % mx
            y = (i//mx) % my
            x = -WIDTH + 2*WIDTH*x/mx
            # rows are packed together, and the whole grid is centered vertically
            row_h = 1.5*scale + GROUP_ROW_GAP
            y = my*row_h/2 - row_h*y

            # face
            username = user["username"]
            face_path = os.path.join(FACE_DIR, username + ".jpg")
            if os.path.exists(face_path):
                img = ImageMobject(face_path)
            else:
                print(f"!!! Face of {username} at {face_path} not found")
                img = ImageMobject(PATH_NO_FACE)
            img.height = 1.5*scale
            img.to_corner(DOWN + LEFT)
            img.set_x(x, LEFT)
            img.set_y(y, UP)

            # Text is shrunk to fit its tile. The medal and the PO circle sit to the right of
            # the name, so the name gets less room; school and city are long and stay small.
            text_w = 2*WIDTH/mx - img.width - 0.2 - 0.3
            name_w = text_w
            if MEDAL in MEDAL_COLORS:
                name_w -= 0.95*scale + 0.25
            if user["po"]:
                name_w -= 0.8*scale + 0.25

            # name
            name = Tex(user["name"])
            name.scale(1.1*scale)
            if name.width > name_w:
                name.width = name_w

            # school
            klass = CLASS[user["class"]]
            school = user["school"]
            city = user["city"]
            province = " (%s)" % user["province"] if user.get("province") else ""

            subsub = Tex(f"{city}{province}")
            subsub.scale(0.5*scale)
            if subsub.width > text_w:
                subsub.width = text_w

            sub = Tex(f"Classe {klass}, {school}")
            sub.scale(0.5*scale)
            if sub.width > text_w:
                sub.width = text_w

            # the three lines are stacked tightly and centered on the face
            text = VGroup(name, sub, subsub).arrange(DOWN, aligned_edge=LEFT, buff=0.12*scale)
            text.set_x(img.get_x(RIGHT) + 0.2, LEFT)
            text.set_y(img.get_y())

            # Fade in
            write_name = Write(name)
            play_list = [
                write_name,
                Write(sub, run_time=write_name.run_time),
                Write(subsub, run_time=write_name.run_time),
                FadeIn(img, shift=RIGHT),
            ]

            # Fade out
            fade_list = [
                FadeOut(name),
                FadeOut(sub),
                FadeOut(subsub),
                FadeOut(img),
            ]

            # medal
            if MEDAL in MEDAL_COLORS:
                medal = ImageMobject(PATH_MEDAL)
                medal.scale(0.25*scale)
                medal.set_color(MEDAL_COLORS[MEDAL])
                medal.next_to(name)
                play_list.append(FadeIn(medal, scale=2))
                fade_list.append(FadeOut(medal))
                if user["position"] != float("inf"):
                    position = Tex(r"\textbf{%d}" % user["position"])
                    position.scale(0.7*scale)
                    position.move_to(medal)
                    position.set_color(BLACK)
                    play_list.append(FadeIn(position, scale=2))
                    fade_list.append(FadeOut(position))

            # po
            if user["po"]:
                po = Circle(stroke_color=WHITE)
                po.scale(0.4*scale)
                po.next_to(medal if MEDAL in MEDAL_COLORS else name)
                po_text = Tex("PO")
                po_text.scale(0.8*scale)
                po_text.move_to(po)
                play_list += [Create(po), Write(po_text)]
                fade_list += [FadeOut(po), FadeOut(po_text)]

            return play_list, fade_list


class Medal(Scene):
    def construct(self):
        self.camera.background_color = BACKGROUND_COLOR
        self.timelapse_dur = TIMELAPSE_DURATION
        self.start_time = [datetime.strptime(s, "%Y-%m-%dT%H:%M:%S") for s in CONTEST_START]
        self.end_time = [datetime.strptime(e, "%Y-%m-%dT%H:%M:%S") for e in CONTEST_END]
        self.timelapse = False
        groups = GROUPS_ARRAY[MEDAL]
        count = 0
        group_count = 0
        group_play = []
        group_fade = []

        logo = ImageMobject(PATH_LOGO)
        logo.scale(0.2)  # logo.png is 1200px tall, so this is 20% smaller than the old 600px logo at 0.5
        logo.to_corner(UP + RIGHT)
        self.add(logo)

        for user in ranking:
            username = user["username"]
            self.position = user["position"]
            print(f"====== Processing {username} ({self.position}) ========")

            if groups:
                p, f = Mention.student_badge(count, user, *groups[0][:3])
                count += 1
                group_play += p
                group_fade += f

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
                when, ext = os.path.splitext(os.path.basename(screen))
                if ext.lower() not in SCREEN_EXTENSIONS:
                    continue
                try:
                    when = datetime.strptime(when, "%Y-%m-%dT%H:%M:%S.%f")
                except ValueError:
                    continue
                for s,e in zip(self.start_time, self.end_time):
                    if when >= s and when <= e:
                        self.screenshots.append((when.timestamp(), screen))
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

            # the updaters must not fire during the animations below (with cached
            # animations they would run on a stale timelapse state and break the score)
            self.screen.clear_updaters()
            self.score.clear_updaters()
            self.progress.clear_updaters()

            # make sure the final score is shown
            self.score.become(self.get_score(-1))
            self.screen.become(self.get_screen(-1))

            # medal
            medal = ImageMobject(PATH_MEDAL)
            medal.scale(0.75)
            medal.set_color(MEDAL_COLORS[MEDAL])
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
            if WINNER_CONFETTI_DURATION and self.position == 1:
                # self.wait(1)
                confetti = get_confetti_animations(150)
                self.add(*confetti)
                self.wait(WINNER_CONFETTI_DURATION)
            else:
                self.wait(MEDAL_DELAY[MEDAL])

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

            if groups and count == group_size(groups[0]):
                group_count += 1
                print(f"====== Group {group_count} ({groups[0]}) ========")
                self.play(FadeOut(logo))
                self.play(*group_play)
                self.wait(2)
                self.play(*group_fade)
                self.play(FadeIn(logo))
                count = 0
                groups = groups[1:]
                group_play = []
                group_fade = []

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
