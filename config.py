#!/usr/bin/env python3

# Reduce for debugging
MAX_USERS = 999

# Contest information
CONTEST_START = ["2025-09-23T09:00:00", "2025-09-24T09:00:00"]
CONTEST_END = ["2025-09-23T14:00:01", "2025-09-24T14:00:01"]
MAX_SCORE = 400

# Timing
MEDAL_DELAY = {
    "gold": 2,
    "silver": 1,
    "bronze": 0.5,
    "mention": 1,
}
TIMELAPSE_DURATION = 4
WINNER_CONFETTI_DURATION = 10 # if 0, no winner confetti at all

# Medal groups
GROUPS_ARRAY = {
    "gold" : None,
    "silver": [(2,3,0.7), (1,5,0.6), (2,2,0.8), (1,3,1.0)],
    "bronze": None,
    "mention": [(2,5,0.7)]
}

# Images and directories
OUTPUT_DIR = "output"
SCREEN_DIR = "screenshots"
FACE_DIR = OUTPUT_DIR + "/faces"
PATH_LOGO = "images/logo.png"
PATH_CUP = "images/cup.svg"
PATH_MEDAL = "images/medal.png"
PATH_NO_FACE = "images/noface.jpg"
PATH_NO_SCREEN = "images/circuits.png"

# General properties
MEDAL_NAMES = {
    "gold": ["gold", "oro"],
    "silver": ["silver", "argento"],
    "bronze": ["bronze", "bronzo"],
    "mention": ["mention", "menzione"]
}
MEDAL_COLORS = {
    "gold": "#FFD700",
    "silver": "#C0C0C0",
    "bronze": "#CC6633",
}
BACKGROUND_COLOR = "#222222"
CLASS = {
    1: "I",
    2: "II",
    3: "III",
    4: "IV",
    5: "V",
    6: "I",
    7: "II",
    8: "III",
    9: "I",
    10: "II",
    11: "III",
    12: "IV",
    13: "V",
}
