import sys
import random
from manim import *

sys.path.append(".")

from config import *


def make_falling_confetti_mobject(x_start, start_delay, duration):
    colors = [RED, YELLOW, GREEN, BLUE, PURPLE, RED]
    square = Square(
        side_length=0.2,
        stroke_width=0,
        fill_opacity=0.75,
        fill_color=random.choice(colors),
    )
    square.next_to(x_start * RIGHT + config.frame_y_radius * UP, UP)

    time = 0
    turns = 2 * random.random()
    turn_width = 0.03 * random.random()

    def update(square, dt):
        nonlocal time
        time += dt

        if time < start_delay:
            return

        t = (time - start_delay) / duration
        x = square.get_x() + turn_width * np.cos(turns * t * TAU)
        y = config.frame_y_radius - config.frame_height * t
        angle = 2 * np.pi * dt
        square.rotate(angle, axis=UP, about_point=square.get_center())
        square.move_to(x * RIGHT + y * UP)

        if time > start_delay + duration * 0.85 and t <= 1:
            square.fade(1 - (1 - t) / (1 - 0.85))

    square.add_updater(update)
    return square


def get_confetti_animations(num_confetti_squares):
    confetti = [
        make_falling_confetti_mobject(
            config.frame_width * random.random() - config.frame_x_radius,
            start_delay=a,
            duration=5,
        )
        for a in np.linspace(0, WINNER_CONFETTI_DURATION, num_confetti_squares)
    ]
    return confetti


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
        for k, v in d.items():
            setattr(self, k, v)
        Animation.__init__(self, mobject, **d)
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
