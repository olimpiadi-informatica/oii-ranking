# OII ranking

Instructions on how to build the videos for the ranking of the OII.

## Requirements

- Python3 + manim + manim's dependencies
- A ranking file `./ranking.csv` with columns: `position`, `username`, `name`, `school`, `city`, `province`, `medal`, `class`, `po`. If the `po` column is missing, no participant will be considered a PO. Position can be empy for unofficial contestants.
- A folder with the cmsRankingWebServer or terry or quizms data (`./ranking/`, mandatory).
- A folder with all the faces (`./faces/username.jpg`). Missing faces are substituted with a placeholder.
- A folder with all the screenshots (`./screenshots/username/date.png`). If screenshots for a user are missing, a placeholder is used instead. If all screenshots are missing, you can auto-generate placeholders if files `./screenshots/background.png` and `./screenshots/filler.png` are given. Auto-generation requires several minutes and will not be performed if files matching pattern `20*00.0.png` are already found in `./screenshots/`. 

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

Update the settings in `./config.py` and run one of the following commands to render the video:

```
manim render -ql ./ranking.py <Medal>  # preview quality
manim render -qh ./ranking.py <Medal>  # final render
```

Medal can be Gold, Silver, Bronze or Mention (case-sensitive).

The rendered video is stored at `./media/videos/slide/`.

In `config.py` you can specify many other properties, including the array configuration for mentions (property `MENTION_ARRAY`).

#### Render overlays

A few handy overlays are also provided: `Countdown` and `NameMarker`. To render them:

```
manim render -qh -t ./overlays.py [overlay]
```

They will prompt for the needed input parameters.
