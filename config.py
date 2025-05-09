#!/usr/bin/env python3

# Contest information
CONTEST_START = "2024-04-23T14:00:00"
CONTEST_END = "2024-04-23T17:00:01"
MAX_SCORE = 200
WINNER_CONFETTI = True

# Debugging
NUM_USERS = 150
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
    "gold": 4,
    "silver": 2,
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
TIMELAPSE_DURATION = 2
WINNER_CONFETTI_DURATION = 10
MENTION_ARRAY = (2,5)
