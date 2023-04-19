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
./preprocess.py
```

Now inside of `./output` there is some preprocessed data. In particular, the images of the faces have been normalized by cropping a square in the center and stored in `./output/faces`. This process is not perfect and can be manually tuned by replacing the images inside that folder. `./preprocess.py` won't overwrite them.

#### Render a medal

Put your settings in `./config.py` and run one of the following commands to render the video:

```
manim ./slide.py Slide -l              # preview quality
manim ./slide.py Slide --high-quality  # final render
```

The rendered video is stored at `./media/videos/slide/`.

> Note that this will render _only_ the contestants with the medal specified in `config.py`. You will need to update and run the above commands three times, one for each medal color.
