from manim import *

class Countdown(Scene):
    def construct(self):
        print("Insert the starting countdown number:", end= ' ')
        N = int(input())
        for i in reversed(range(N + 1)):
            t = Text(("0" if i<10 else "") + str(i), font="OCR A Extended", font_size=144)
            self.play(FadeIn(t, scale=0, run_time=0.5))
            self.play(FadeOut(t, scale=5, run_time=0.5))

class NameMarker(Scene):
    def construct(self):
        print("Insert the full name of the person:", end= ' ')
        name = input().strip()
        print("Insert the title of the person:", end= ' ')
        title = input().strip()
        r = Rectangle(height = 1, width = 8)
        r.set_fill("#C0C0C0", opacity = 0.8)
        r.set_stroke(width = 0)
        r.to_edge(RIGHT, buff=0)
        r.to_edge(DOWN, buff=0.5)
        n = Text(name, font="Futura", font_size=36, color=BLACK)
        t = Text(title, font="Futura", font_size=24, color=BLACK)
        g = VGroup()
        g.add(n)
        g.add(t)
        g.arrange(DOWN, center=False, aligned_edge=LEFT, buff=0)
        g.to_edge(LEFT, buff=6.4)
        g.to_edge(DOWN, buff=0.55)
        self.play(Wait(0.5))
        self.play(Write(r))
        self.play(LaggedStart(Write(n), Write(t), lag_ratio=0.3))
        self.play(Wait(3))
        self.play(LaggedStart(Unwrite(t), Unwrite(n), lag_ratio=0.3))
        self.play(Unwrite(r))
        self.play(Wait(0.5))
