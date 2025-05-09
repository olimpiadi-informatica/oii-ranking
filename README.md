# OII ranking

Instructions on how to build the videos for the ranking of the OII.

## Requirements

- Python3 + manim + manim's dependencies
- A ranking file `./ranking.csv` with columns: `position`, `username`, `name`, `school`, `city`, `province`, `medal`, `class`, `po`. If the `po` column is missing, no participant will be considered a PO.
- A folder with the cmsRankingWebServer or terry or quizms data (`./ranking/`)
- A folder with all the faces (`./faces/username.jpg`)
- A folder with all the screenshots (`./screenshots/username/date.png`). If screenshots are missing, you can auto-generate placeholders if files `./screenshots/background.png` and `./screenshots/filler.png` are given. Auto-generation requires several minutes and will not be performed if files matching pattern `20*00.0.png` are already found in `./screenshots/`.

## Instructions

#### Install manim

```
mkvirtualenv manim
pip install manim
```

#### Preprocess data

For cms data:

```
./preprocess.py
```

For terry and/or quizms data:

```
./preprocess.py -t
```

Now inside of `./output` there is some preprocessed data. In particular, the images of the faces have been normalized by cropping a square in the center and stored in `./output/faces`. This process is not perfect and can be manually tuned by replacing the images inside that folder. `./preprocess.py` won't overwrite them.

#### Render a medal

Put your settings in `./config.py` and run one of the following commands to render the video:

```
manim render -ql ./slide.py Slide  # preview quality
manim render -qh ./slide.py Slide  # final render
```

If you want to render a medal in mention-style, without showing scores and positions (and with multiple faces per page), you run the following similar command:

```
manim render -ql ./mention.py Mention  # preview quality
manim render -qh ./mention.py Mention  # final render
```

The rendered video is stored at `./media/videos/slide/`.

> Note that this will render _only_ the contestants with the medal specified in `config.py`. You will need to update and run the above commands three (or four) times, one for each medal color. In `config.py` you also specify the array configuration for mentions (property `MENTION_ARRAY`).

#### Render overlays

A few handy overlays are also provided: `Countdown` and `NameMarker`. To render them:

```
manim render -qh -t ./overlays.py [overlay]
```

They will prompt for the needed input parameters.
