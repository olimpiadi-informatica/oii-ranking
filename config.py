#!/usr/bin/env python3

# Contest information
CONTEST_START = "2021-11-16T13:30:00"
CONTEST_END = "2021-11-16T18:30:01"
MAX_SCORE = 400
WINNER_CONFETTI = False

# Debugging
NUM_USERS = 3
MEDAL = "gold"

# Data on disk
PATH_LOGO = "./noface.png"
PATH_CUP = "./cup.svg"
PATH_OUTPUT = "./output"
SCREEN_DIR = "./screenshots"
FACE_DIR = PATH_OUTPUT + "/faces"
PATH_MEDAL = "./medal.png"
PATH_NO_FACE = "./noface.png"

# Customizations
MEDAL_COLORS = {
    "gold": "#FFD700",
    "silver": "#C0C0C0",
    "bronze": "#CC6633",
}
BACKGROUND_COLOR = "#222222"
MEDAL_DELAY = {
    "gold": 5,
    "silver": 3,
    "bronze": 3,
}
CLASS = {
    1: "I",
    2: "II",
    3: "III",
    4: "IV",
    5: "V",
}
TIMELAPSE_DURATION = 5
