#!/usr/bin/env python3

import argparse
import csv
import subprocess
import glob
import json
import logging
import os.path
from datetime import datetime
import config
import subprocess

logger = logging.getLogger("preprocess")


def get_submissions_data(dir: str):
    submissions = {}
    for path in glob.glob(os.path.join(dir, "*")):
        id = os.path.basename(path)[: -len(".json")]
        with open(path, "r") as f:
            data = json.load(f)
            submissions[id] = data
    return submissions


def get_subchanges_data(dir: str):
    subchanges = []
    for path in glob.glob(os.path.join(dir, "*")):
        with open(path, "r") as f:
            data = json.load(f)
            subchanges.append(data)
    return subchanges

def get_terry_data(dir: str):
    submissions = {}
    subchanges = []
    for path in glob.glob(os.path.join(dir, "*", "*", "*", "info.txt")):
        with open(path, "r") as f:
            date, score = f.readlines()
        date = int(datetime.strptime(date[6:-1], "%Y-%m-%d %H:%M:%S").timestamp())
        score = int(float(score[7:-1]))
        p = os.path.dirname(path)
        p, sub = os.path.split(p)
        p, user = os.path.split(p)
        task = os.path.basename(p)
        sub = path
        submissions[sub] = {
            "user" : user,
            "task" : task,
            "time" : date,
        }
        subchanges.append({
            "submission" : sub,
            "time" : date,
            "score" : score,
            "extra" : [score],
        })
    return submissions, subchanges

def fake_screenshots(dir: str, usernames, start, end):
    screens = glob.glob(os.path.join(dir, "20*00.0.png"))
    if len(screens) == 0:
        subprocess.run(
            ["asy", "clock.asy", "-f", "png", "-globalwrite", "-antialias", "4"],
            input=bytes(config.CONTEST_START + " " + config.CONTEST_END, "utf-8")
        )
        screens = glob.glob(os.path.join(dir, "20*00.0.png"))
    for user in usernames:
        if not os.path.exists(os.path.join(dir, user)):
            os.makedirs(os.path.join(dir, user))
        for path in screens:
            file = os.path.basename(path)
            if not os.path.exists(os.path.join(dir, user, file)):
                os.symlink(os.path.join("..", file), os.path.join(dir, user, file))

def main(args):
    if not os.path.exists(args.ranking_dir):
        raise RuntimeError("Missing ranking directory")
    os.makedirs(args.output_dir, exist_ok=True)

    if args.terry:
        submissions, subchanges = get_terry_data(args.ranking_dir)
    else:
        submissions = get_submissions_data(os.path.join(args.ranking_dir, "submissions"))
        subchanges = get_subchanges_data(os.path.join(args.ranking_dir, "subchanges"))

    subchanges.sort(key=lambda x: x["time"])

    logger.info("%s submissions", len(submissions))
    logger.info("%s subchanges", len(subchanges))

    user_task_score = {}
    user_score_history = {}
    for subch in subchanges:
        sub = submissions[subch["submission"]]
        username = sub["user"]
        task = sub["task"]
        subtasks = list(map(float, subch["extra"]))
        time = subch["time"]

        user = user_task_score.setdefault(username, {})
        if task not in user:
            user[task] = subtasks
        else:
            for i, score in enumerate(subtasks):
                user[task][i] = max(score, user[task][i])

        history = user_score_history.setdefault(username, [])
        score = sum(sum(subtasks) for subtasks in user.values())
        history.append(dict(time=time, score=score))

    logger.info("%s users have submitted", len(user_task_score))

    with open(os.path.join(args.output_dir, "final_scores.json"), "w") as f:
        json.dump(user_task_score, f, indent=4)

    with open(os.path.join(args.output_dir, "history.json"), "w") as f:
        json.dump(user_score_history, f, indent=4)

    with open(args.ranking_csv, "r") as f:
        reader = csv.DictReader(f)
        ranking = list(reader)
        for user in ranking:
            user["po"] = user["po"] != "" and user["po"] != "F"
            user["position"] = int(user["position"])
            user["class"] = int(user["class"])

    with open(os.path.join(args.output_dir, "ranking.json"), "w") as f:
        json.dump(ranking, f, indent=4)

    if args.terry:
        fake_screenshots("./screenshots", [user["username"] for user in ranking if user["medal"]], config.CONTEST_START, config.CONTEST_END)

    logger.info("Resizing faces...")
    os.makedirs(os.path.join(args.output_dir, "faces"), exist_ok=True)
    for path in glob.glob(os.path.join(args.faces_dir, "*")):
        name = os.path.basename(path)
        name, _ = os.path.splitext(name)
        target = os.path.join(args.output_dir, "faces", name + ".jpg")
        if os.path.exists(target):
            continue
        subprocess.check_call(
            [
                "convert",
                path,
                "-resize",
                "1000x1000^",
                "-gravity",
                "Center",
                "-extent",
                "1000x1000",
                target,
            ]
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--faces-dir",
        help="directory with the faces",
        default="./faces/",
    )
    parser.add_argument(
        "--ranking-dir",
        help="directory with the ranking",
        default="./ranking/",
    )
    parser.add_argument(
        "--ranking-csv",
        help="path to ranking.csv",
        default="./ranking.csv",
    )
    parser.add_argument(
        "--output-dir",
        help="wirectory to write the output",
        default="./output/",
    )
    parser.add_argument(
        "--terry", "-t",
        help="import data in terry format",
        action="store_true",
    )
    parser.add_argument(
        "--verbose", "-v",
        help="enable verbose output",
        action="store_true",
    )
    args = parser.parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] [%(name)s] %(message)s",
    )
    main(args)
