#!/usr/bin/env python3
"""Generate placeholder data (cms format) for a quick smoke test of the tool.

Creates ./ranking.csv, ./ranking/, ./faces/ and ./screenshots/, all of which are
git-ignored. Refuses to run if any of them already exists.

Usage: .venv/bin/python make_test_data.py && .venv/bin/python preprocess.py
"""

import csv
import json
import os
import random
from datetime import datetime, timedelta

from PIL import Image, ImageDraw

import config

random.seed(0)

# (medal, how many)
MEDALS = [("oro", 3), ("argento", 6), ("bronzo", 2), ("menzione", 4)]
PROVINCES = ["MI", "RM", "TO", "NA", "PI", "BO"]
TARGETS = ["ranking.csv", "ranking", "faces", "screenshots"]

for t in TARGETS:
    if os.path.exists(t):
        raise SystemExit(f"{t} already exists, refusing to overwrite it")

starts = [datetime.strptime(s, "%Y-%m-%dT%H:%M:%S") for s in config.CONTEST_START]
ends = [datetime.strptime(e, "%Y-%m-%dT%H:%M:%S") for e in config.CONTEST_END]

# ---- users -------------------------------------------------------------
users = []
for medal, n in MEDALS:
    for _ in range(n):
        i = len(users) + 1
        users.append(
            {
                "position": i,
                "username": f"user{i:02d}",
                "name": f"Nome{i:02d} Cognome{i:02d}",
                "school": f"Liceo Placeholder {i}",
                "city": f"Citta{i:02d}",
                "province": random.choice(PROVINCES),
                "medal": medal,
                "class": random.randint(1, 5),
                "po": "T" if i in (1, 5) else "F",
                "final": max(0, config.MAX_SCORE - 25 * i),
            }
        )
# one unofficial contestant (empty position) with a medal
users[-1]["position"] = ""

with open("ranking.csv", "w", newline="") as f:
    cols = ["position", "username", "name", "school", "city", "province", "medal", "class", "po"]
    w = csv.DictWriter(f, fieldnames=cols, extrasaction="ignore")
    w.writeheader()
    w.writerows(users)

# ---- cms ranking data: submissions + subchanges --------------------------
os.makedirs("ranking/submissions")
os.makedirs("ranking/subchanges")
sid = 0
for u in users:
    # score grows in 4 steps spread over the contest days
    steps = sorted(random.sample(range(1, 100), 4))
    for k, s in enumerate(steps):
        day = k * len(starts) // len(steps)
        t = starts[day] + (ends[day] - starts[day]) * (s / 100)
        score = u["final"] * (k + 1) / len(steps)
        sub = f"sub{sid:04d}"
        with open(f"ranking/submissions/{sub}.json", "w") as f:
            json.dump({"user": u["username"], "task": "task1", "time": int(t.timestamp())}, f)
        with open(f"ranking/subchanges/{sub}.json", "w") as f:
            json.dump(
                {"submission": sub, "time": int(t.timestamp()), "score": score, "extra": [score]}, f
            )
        sid += 1

# ---- faces ---------------------------------------------------------------
os.makedirs("faces")
for u in users:
    img = Image.new("RGB", (400, 300), tuple(random.randint(60, 220) for _ in range(3)))
    d = ImageDraw.Draw(img)
    d.ellipse((130, 60, 270, 200), fill=(240, 220, 200))
    d.text((10, 10), u["username"], fill="white")
    img.save(f"faces/{u['username']}.jpg")

# ---- screenshots: a few shared frames, symlinked per user ----------------
os.makedirs("screenshots")
frames = []
for s, e in zip(starts, ends):
    for frac in (0, 0.5, 1):
        when = s + (e - s) * frac
        name = when.strftime("%Y-%m-%dT%H:%M:%S") + ".0.png"
        img = Image.new("RGB", (480, 270), (30, 40, 60))
        ImageDraw.Draw(img).text((20, 120), when.isoformat(), fill="white")
        img.save(os.path.join("screenshots", name))
        frames.append(name)
for u in users:
    d = os.path.join("screenshots", u["username"])
    os.makedirs(d)
    for name in frames:
        os.symlink(os.path.join("..", name), os.path.join(d, name))

print(f"Created placeholder data for {len(users)} users. Now run ./preprocess.py")
