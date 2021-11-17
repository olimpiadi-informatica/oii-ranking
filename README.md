# OII ranking

Instructions on how to build the videos for the ranking of the OII.

## Requirements

- Python3 + manim + manim's dependencies
- A folder with the cmsRankingWebServer data (`./ranking/`)
- A folder with all the faces (`./faces/username.jpg`)
- A folder with all the screenshots (`./screenshots/username/date.png`)
- A CSV with the ranking (at least with: `position`, `username`, `name`, `school`, `city`, `province`, `medal`, `class`, `po`)

## Instructions

#### Install manimlib

```
mkvirtualenv manim
pip install manimlib
```

#### Preprocess data

```
./preprocess
```
