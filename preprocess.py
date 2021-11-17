#!/usr/bin/env python3

import argparse
import csv
import subprocess
import glob
import json
import logging
import os.path

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


def main(args):
    if not os.path.exists(args.ranking_dir):
        raise RuntimeError("Missing ranking directory")
    os.makedirs(args.output_dir, exist_ok=True)

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
        help="Directory with the faces",
        default="./faces/",
    )
    parser.add_argument(
        "--ranking-dir",
        help="Directory with the ranking",
        default="./ranking/",
    )
    parser.add_argument(
        "--ranking-csv",
        help="Path to ranking.csv",
        default="./ranking.csv",
    )
    parser.add_argument(
        "--output-dir",
        help="Directory to write the output",
        default="./output/",
    )
    parser.add_argument("--verbose", "-v", action="store_true")
    args = parser.parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] [%(name)s] %(message)s",
    )
    main(args)
