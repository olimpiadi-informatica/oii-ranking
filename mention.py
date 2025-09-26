import sys
import json
import os.path
import random
from datetime import datetime
from glob import glob

from manim import *

WIDTH, HEIGHT = config.frame_x_radius, config.frame_y_radius
WIDTH -= 0.5
HEIGHT -= 0.5

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
    OUTPUT_DIR,
    SCREEN_DIR,
    TIMELAPSE_DURATION,
    WINNER_CONFETTI,
    WINNER_CONFETTI_DURATION,
    MENTION_ARRAY,
)

with open(os.path.join(OUTPUT_DIR, "ranking.json")) as f:
    ranking = json.load(f)


class Mention(Scene):
    def construct(self):
        self.camera.background_color = BACKGROUND_COLOR
        self.timelapse_dur = TIMELAPSE_DURATION
        self.start_time = datetime.strptime(CONTEST_START, "%Y-%m-%dT%H:%M:%S")
        self.end_time = datetime.strptime(CONTEST_END, "%Y-%m-%dT%H:%M:%S")
        self.timelapse = False

        if NUM_USERS is not None:
            num_users = NUM_USERS
        else:
            num_users = len(ranking)
        users = [user for user in ranking[:num_users] if user["medal"] == MEDAL]

        ARRX, ARRY = MENTION_ARRAY
        BUNCH = ARRX * (ARRY-1)
        fade_list = []
        for i,user in enumerate(users):
            username = user["username"]
            print(f"====== Processing {username} ========")
            x = i % ARRX
            y = (i//ARRX) % ARRY

            scale = 0.7

            # face
            face_path = os.path.join(FACE_DIR, username + ".jpg")
            if os.path.exists(face_path):
                img = ImageMobject(face_path)
                img.height = 1.5*scale
                img.to_corner(DOWN + LEFT)
                img.set_x(-WIDTH + 2*WIDTH*x/ARRX, LEFT)
                img.set_y(HEIGHT - 2*HEIGHT*y/ARRY, UP)
            else:
                print(f"!!! Face of {username} at {face_path} not found")
                img = ImageMobject(PATH_NO_FACE)

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
