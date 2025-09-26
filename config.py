#!/usr/bin/env python3

# Contest information
CONTEST_START = ["2025-09-23T09:00:00", "2025-09-24T09:00:00"]
CONTEST_END = ["2025-09-23T14:00:01", "2025-09-24T14:00:01"]
MAX_SCORE = 400
WINNER_CONFETTI = True

# Debugging
NUM_USERS = 150
MEDAL = "gold"

# Data on disk
OUTPUT_DIR = "output"
SCREEN_DIR = "screenshots"
FACE_DIR = OUTPUT_DIR + "/faces"
PATH_LOGO = "images/logo.png"
PATH_CUP = "images/cup.svg"
PATH_MEDAL = "images/medal.png"
PATH_NO_FACE = "images/noface.jpg"
PATH_NO_SCREEN = "images/circuits.png"

# Customizations
MEDAL_COLORS = {
    "gold": "#FFD700",
    "silver": "#C0C0C0",
    "bronze": "#CC6633",
}
BACKGROUND_COLOR = "#222222"
MEDAL_DELAY = {
    "gold": 2,
    "silver": 1,
    "bronze": 0.5,
}
CLASS = {
    1: "I",
    2: "II",
    3: "III",
    4: "IV",
    5: "V",
    6: "I",
    7: "II",
    8: "III",
}
TIMELAPSE_DURATION = 4
WINNER_CONFETTI_DURATION = 10
MENTION_ARRAY = (2,5)
