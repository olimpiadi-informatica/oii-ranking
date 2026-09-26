"""64:9 version of ranking.py.

manim render -ql|-qm|-qh ranking_wide.py <Gold|Silver|Bronze|Mention>

The video height is set by the quality flag; the width follows from ASPECT_* in
config_wide.py (e.g. 7680x1080 with -qh).
"""

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

sys.path.append(".")

# confetti re-exports the values of config.py, so config_wide must be imported after it
from confetti import *
from config_wide import *

# Make the frame wide. The pixel width must be even for the h264 encoder.
config.pixel_width = 2 * round(config.pixel_height * ASPECT_WIDTH / ASPECT_HEIGHT / 2)
config.frame_width = config.frame_height * config.pixel_width / config.pixel_height

FRAME_W, FRAME_H = config.frame_width, config.frame_height
WIDTH, HEIGHT = FRAME_W / 2 - 0.5, FRAME_H / 2 - 0.5

# Horizontal zones of the medal scenes: name -> (center x, width)
ZONES = {}
_left = -FRAME_W / 2
for _name, _fraction in ZONE_FRACTIONS.items():
    ZONES[_name] = (_left + FRAME_W * _fraction / 2, FRAME_W * _fraction)
    _left += FRAME_W * _fraction

# The medal scenes of ranking.py show the medal at scale 0.75; the PO circle and the position
# number are scaled by the same factor as the medal relative to that
MEDAL_K = MEDAL_SCALE / 0.75

# Vertical extent of the content, below the score bar
CONTENT_TOP = FRAME_H / 2 - BAR_HEIGHT - MARGIN
CONTENT_BOTTOM = -FRAME_H / 2 + MARGIN
CONTENT_CY = (CONTENT_TOP + CONTENT_BOTTOM) / 2

with open(os.path.join(OUTPUT_DIR, "history.json")) as f:
    history = json.load(f)

with open(os.path.join(OUTPUT_DIR, "ranking.json")) as f:
    ranking = json.load(f)


def final_score(user):
    h = history.get(user["username"])
    return h[-1]["score"] if h else 0


# Score from which the top bar takes the color of each medal: the lowest score among the
# official contestants who got that medal
MEDAL_THRESHOLDS = {}
for u in ranking:
    if u["position"] == float("inf"):
        continue
    for m, names in MEDAL_NAMES.items():
        if m in MEDAL_COLORS and u["medal"].lower() in names:
            MEDAL_THRESHOLDS[m] = min(MEDAL_THRESHOLDS.get(m, float("inf")), final_score(u))


def bar_color(points):
    for m in ("gold", "silver", "bronze"):
        if m in MEDAL_THRESHOLDS and points >= MEDAL_THRESHOLDS[m]:
            return MEDAL_COLORS[m]
    return BAR_COLOR_NO_MEDAL


# The scene name is found by value, so extra manim options (e.g. --media_dir) can go anywhere
MEDAL = next((a.lower() for a in sys.argv[1:] if a.lower() in MEDAL_NAMES), None)
if MEDAL is None:
    raise SystemExit("Usage: manim render [options] ranking_wide.py <Gold|Silver|Bronze|Mention>")
ranking = [u for u in reversed(ranking) if u["medal"].lower() in MEDAL_NAMES[MEDAL]][-MAX_USERS:]

print('Rendering ranking of %d students with medal "%s" at %dx%d...\n'
      % (len(ranking), MEDAL, config.pixel_width, config.pixel_height))


def group_size(group):
    """Number of contestants of a group: the optional 4th element, else columns * rows."""
    return group[3] if len(group) > 3 else group[0] * group[1]


# Horizontal extent between the two logo zones
CENTER_X_RANGE = (
    ZONES["logo_left"][0] + ZONES["logo_left"][1] / 2 + MARGIN,
    ZONES["logo_right"][0] - ZONES["logo_right"][1] / 2 - MARGIN,
)


def group_area(group):
    """x and y ranges of the recap grid of a group: sized by its scale, centered between the logos."""
    cols, rows, scale = group[:3]
    tile_w = min(GROUP_TILE_WIDTH * scale, (CENTER_X_RANGE[1] - CENTER_X_RANGE[0]) / cols)
    row_h = 1.5 * scale + GROUP_ROW_GAP
    cx = (CENTER_X_RANGE[0] + CENTER_X_RANGE[1]) / 2
    return (
        (cx - cols * tile_w / 2, cx + cols * tile_w / 2),
        (CONTENT_CY - rows * row_h / 2, CONTENT_CY + rows * row_h / 2),
    )


def get_logo(zone):
    """Logo centered in one of the two logo zones."""
    cx, zw = ZONES[zone]
    logo = ImageMobject(PATH_LOGO)
    logo.height = (CONTENT_TOP - CONTENT_BOTTOM) * LOGO_SCALE
    if logo.width > (zw - 2 * MARGIN) * LOGO_SCALE:
        logo.width = (zw - 2 * MARGIN) * LOGO_SCALE
    logo.move_to([cx, CONTENT_CY, 0])
    return logo


def student_badge(i, user, mx, my, scale, x_range=None, y_range=None):
    """Small card (face, name, school, city, medal, PO) for slot i of a mx x my grid.

    The grid spans x_range = (left, right) and y_range = (bottom, top), the whole frame by
    default. Text that would not fit in its tile is shrunk."""
    x_left, x_right = x_range if x_range else (-WIDTH, WIDTH)
    y_bottom, y_top = y_range if y_range else (-HEIGHT, HEIGHT)
    cell_w, cell_h = (x_right - x_left) / mx, (y_top - y_bottom) / my
    x = x_left + cell_w * (i % mx)
    y = y_top - cell_h * ((i // mx) % my + 0.5)  # vertical center of the cell

    # face
    username = user["username"]
    face_path = os.path.join(FACE_DIR, username + ".jpg")
    if os.path.exists(face_path):
        img = ImageMobject(face_path)
    else:
        print(f"!!! Face of {username} at {face_path} not found")
        img = ImageMobject(PATH_NO_FACE)
    img.height = 1.5 * scale
    img.set_x(x, LEFT)
    img.set_y(y)

    # name
    # (the medal and the PO circle sit to the right of the name, so they need room too)
    text_w = cell_w - img.width - 0.2 - 0.3
    name_w = text_w - (1.3 * scale if MEDAL in MEDAL_COLORS else 0) - (0.9 * scale if user["po"] else 0)

    name = Tex(user["name"])
    name.scale(1.1 * scale)
    if name.width > name_w:
        name.width = name_w

    # school
    klass = CLASS[user["class"]]
    school = user["school"]
    city = user["city"]
    province = " (%s)" % user["province"] if user.get("province") else ""

    subsub = Tex(f"{city}{province}")
    subsub.scale(0.5 * scale)
    if subsub.width > text_w:
        subsub.width = text_w

    sub = Tex(f"Classe {klass}, {school}")
    sub.scale(0.5 * scale)
    if sub.width > text_w:
        sub.width = text_w

    # the three lines are stacked tightly and centered on the face
    text = VGroup(name, sub, subsub).arrange(DOWN, aligned_edge=LEFT, buff=0.12 * scale)
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
        medal.scale(0.25 * scale)
        medal.set_color(MEDAL_COLORS[MEDAL])
        medal.next_to(name)
        play_list.append(FadeIn(medal, scale=2))
        fade_list.append(FadeOut(medal))
        if user["position"] != float("inf"):
            position = Tex(r"\textbf{%d}" % user["position"])
            position.scale(0.7 * scale)
            position.move_to(medal)
            position.set_color(BLACK)
            play_list.append(FadeIn(position, scale=2))
            fade_list.append(FadeOut(position))

    # po
    if user["po"]:
        po = Circle(stroke_color=WHITE)
        po.scale(0.4 * scale)
        po.next_to(medal if MEDAL in MEDAL_COLORS else name)
        po_text = Tex("PO")
        po_text.scale(0.8 * scale)
        po_text.move_to(po)
        play_list += [Create(po), Write(po_text)]
        fade_list += [FadeOut(po), FadeOut(po_text)]

    return play_list, fade_list


class Mention(Scene):
    def construct(self):
        self.camera.background_color = BACKGROUND_COLOR

        # the logos stay on the sides, the badges are tiled in between
        logos = [get_logo("logo_left"), get_logo("logo_right")]
        self.add(*logos)
        x_range = CENTER_X_RANGE

        ARRX, ARRY, SCALE = GROUPS_ARRAY["mention"][0]
        # a badge is faded out only when its slot is needed again
        BUNCH = ARRX * ARRY
        fade_list = []
        for i, user in enumerate(ranking):
            username = user["username"]
            print(f"====== Processing {username} ========")

            play_list, fade_bunch = student_badge(i, user, ARRX, ARRY, SCALE, x_range)
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

        # everything that is still on screen goes away together
        self.wait(2)
        self.play(*[anim for fades in fade_list for anim in fades], *[FadeOut(logo) for logo in logos])
        self.wait(1)


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

        logos = [get_logo("logo_left"), get_logo("logo_right")]
        self.add(*logos)

        for user in ranking:
            username = user["username"]
            self.position = user["position"]
            print(f"====== Processing {username} ({self.position}) ========")

            if groups:
                p, f = student_badge(count, user, *groups[0][:3], *group_area(groups[0]))
                count += 1
                group_play += p
                group_fade += f

            self.screen_dir = os.path.join(SCREEN_DIR, username)
            self.history = [{"time": 0, "score": 0.0}, *history[username]]

            # cup
            if self.position <= 3:
                self.wait(2)
                self.cup(self.position)

            # name, school, city, face
            info = self.get_info(user, username)

            # fade in the name and the picture
            self.wait(1)
            name, *lines, img = info
            write_name = Write(name)
            self.play(
                write_name,
                *[Write(line, run_time=write_name.run_time) for line in lines],
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
                for s, e in zip(self.start_time, self.end_time):
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
            self.bar = self.get_bar(0)
            self.bar_points = 0.0  # what the bar currently shows
            self.bar_target = 0.0  # the score it is moving towards

            self.play(FadeIn(self.screen), FadeIn(self.score), FadeIn(self.bar), run_time=0.3)

            def screen_updater(screen):
                if self.find_cur_screen(self.cur_datetime):
                    screen.become(self.get_screen(self.cur_screen))

            self.screen.add_updater(screen_updater)

            def score_updater(score):
                if self.find_cur_score(self.cur_datetime):
                    score.become(self.get_score(self.cur_score))
                    self.bar_target = self.history[self.cur_score]["score"]

            self.score.add_updater(score_updater)

            def bar_updater(bar, dt):
                # exponential approach to the target: smooth, and never overshooting
                self.bar_points += (self.bar_target - self.bar_points) * (1 - np.exp(-BAR_SPEED * dt))
                bar.become(self.get_bar(self.bar_points))

            self.bar.add_updater(bar_updater)

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

            # the updaters must not fire during the animations below
            self.screen.clear_updaters()
            self.score.clear_updaters()
            self.progress.clear_updaters()
            self.bar.clear_updaters()

            # make sure the final score is shown (the bar catches up during the next animation)
            self.score.become(self.get_score(-1))
            self.screen.become(self.get_screen(-1))

            # medal, in the place of the screenshot
            screen_x = ZONES["screen"][0]
            medal = ImageMobject(PATH_MEDAL)
            medal.scale(MEDAL_SCALE)
            medal.set_color(MEDAL_COLORS[MEDAL])
            medal.move_to([screen_x, CONTENT_CY, 0])

            # confetti boom
            confetti_anim = get_confetti_boom_animations(
                medal.get_x(), medal.get_y(), 50
            )
            position_anim = [FadeIn(medal, scale=2)]
            if self.position != float("inf"):
                position = Tex(r"\textbf{%d}" % self.position)
                position.scale(2 * MEDAL_K)
                position.move_to(medal)
                position.set_color(BLACK)
                position_anim.append(FadeIn(position, scale=2))

            bar_from, bar_to = self.bar_points, self.history[-1]["score"]
            self.play(
                UpdateFromAlphaFunc(
                    self.bar,
                    lambda bar, alpha: bar.become(self.get_bar(bar_from + (bar_to - bar_from) * alpha)),
                ),
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
            po.scale(0.5 * MEDAL_K)
            po.next_to(medal)
            po_text = Tex("PO")
            po_text.scale(MEDAL_K)
            po_text.move_to(po)
            if user["po"]:
                self.play(
                    Create(po),
                    Write(po_text),
                )

            # winner confetti (the number of squares follows the width of the frame)
            confetti = []
            if WINNER_CONFETTI_DURATION and self.position == 1:
                confetti = get_confetti_animations(round(150 * FRAME_W / (FRAME_H * 16 / 9)))
                self.add(*confetti)
                self.wait(WINNER_CONFETTI_DURATION)
            else:
                self.wait(MEDAL_DELAY[MEDAL])

            # Fade out
            fade_outs = [
                *[FadeOut(m) for m in info],
                FadeOut(self.score),
                FadeOut(self.bar),
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
                self.play(*group_play)
                self.wait(2)
                self.play(*group_fade)
                count = 0
                groups = groups[1:]
                group_play = []
                group_fade = []

        self.play(*[FadeOut(logo) for logo in logos])

    def get_info(self, user, username):
        """Name, class + school + city and face, stacked and centered in the info zone.

        Returns [name, line, face]."""
        cx, zw = ZONES["info"]
        max_w = zw - 2 * MARGIN

        def fit(tex):
            if tex.width > max_w:
                tex.width = max_w
            return tex

        klass = CLASS[user["class"]]
        province = " (%s)" % user["province"] if user.get("province") else ""

        name = fit(Tex(user["name"]).scale(NAME_SCALE))
        line = fit(Tex(f"Classe {klass}, {user['school']}, {user['city']}{province}").scale(INFO_SCALE))

        face_path = os.path.join(FACE_DIR, username + ".jpg")
        if os.path.exists(face_path):
            img = ImageMobject(face_path)
        else:
            print(f"!!! Face of {username} at {face_path} not found")
            img = ImageMobject(PATH_NO_FACE)

        # the face takes the height left by the text; the whole stack is centered
        gaps = (0.35, 0.5, 0)
        text_h = name.height + line.height + sum(gaps)
        img.height = min(CONTENT_TOP - CONTENT_BOTTOM - text_h, max_w)
        items = [name, line, img]
        y = CONTENT_CY + (text_h + img.height) / 2
        for item, gap in zip(items, gaps):
            item.set_x(cx).set_y(y, UP)
            y -= item.height + gap

        return items

    def get_screen(self, index):
        screen = ImageMobject(self.screenshots[index][1] if len(self.screenshots) else PATH_NO_SCREEN)
        cx, zw = ZONES["screen"]
        max_w = zw - 2 * MARGIN
        max_h = CONTENT_TOP - CONTENT_BOTTOM - 0.5  # room for the progress bar
        screen.width = max_w
        if screen.height > max_h:
            screen.height = max_h
        screen.move_to([cx, CONTENT_CY, 0])
        return screen

    def get_score(self, index):
        points = int(self.history[index]["score"])
        score = Tex(f"{points} / {MAX_SCORE}")
        score.scale(SCORE_SCALE)
        score.move_to([ZONES["score"][0], CONTENT_CY, 0])
        return score

    def get_bar(self, points):
        fraction = min(max(points / MAX_SCORE, 0), 1)
        bar = Rectangle(
            width=max(FRAME_W * fraction, 0.001),
            height=BAR_HEIGHT,
            fill_color=bar_color(points),
            fill_opacity=1,
            stroke_width=0,
        )
        bar.set_x(-FRAME_W / 2, LEFT)
        bar.set_y(FRAME_H / 2, UP)
        return bar

    def find_cur_screen(self, now):
        changed = False
        while self.cur_screen + 1 < len(self.screenshots):
            if self.screenshots[self.cur_screen + 1][0] <= now.timestamp():
                self.cur_screen += 1
                changed = True
            else:
                break
        return changed

    def find_cur_score(self, now: datetime):
        changed = False
        while self.cur_score + 1 < len(self.history):
            if self.history[self.cur_score + 1]["time"] <= now.timestamp():
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
            i = min(int(now_perc * len(self.start_time)), len(self.start_time) - 1)
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
