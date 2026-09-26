#!/usr/bin/env python3

# Settings for ranking_wide.py (64:9 layout). Everything not overridden here (contest
# dates, timing, colors, paths, ...) is inherited from config.py.
from config import *

# Aspect ratio of the video; the height is set by the manim quality flag (-ql, -qm, -qh)
ASPECT_WIDTH = 64
ASPECT_HEIGHT = 9

# Horizontal layout of the medal scenes, left to right, as fractions of the width
# (they must sum to 1)
ZONE_FRACTIONS = {
    "logo_left": 3 / 16,
    "info": 1 / 4,
    "score": 1 / 8,
    "screen": 1 / 4,
    "logo_right": 3 / 16,
}

# Top score bar
BAR_HEIGHT = 0.5
BAR_COLOR_NO_MEDAL = "#3B82F6"
BAR_SPEED = 6.0  # how fast the bar follows the score, in 1/s (higher = snappier)

# Sizes inside the medal scenes (manim units; the frame is 8 units tall)
MARGIN = 0.4  # vertical margin of the content, and horizontal margin inside a zone
LOGO_SCALE = 0.9  # size of the logos relative to the space they have
SCORE_SCALE = 2.4
NAME_SCALE = 2.2  # name in the info zone
INFO_SCALE = 1.0  # class, school and city (one line) in the info zone
MEDAL_SCALE = 1.0

# Medal groups (recap pages): (columns, rows, scale[, contestants]), as in config.py. The
# recap is a grid centered between the two logos, sized by its scale (see GROUP_* below);
# the mentions are the exception: they are tiled between the logos over their whole width.
GROUPS_ARRAY = {
    "gold": [(2, 3, 1.2, 5), (2, 2, 1.5, 4)],  # last 5 golds, then the top 4
    "silver": [(2, 3, 1.2)] * 3,  # 18 silvers, 6 at a time
    "bronze": [(2, 5, 0.65)] * 3,  # 30 bronzes, 10 at a time
    "mention": [(5, 5, 0.7)],
}
GROUP_TILE_WIDTH = 10.5  # width of one recap tile at scale 1
GROUP_ROW_GAP = 0.3  # vertical space between recap rows, on top of the face height
