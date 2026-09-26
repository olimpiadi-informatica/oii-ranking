# OII ranking

Instructions on how to build the videos for the ranking of the OII.

## Requirements

- Python3 + manim + manim's dependencies
- A ranking file `./ranking.csv` with columns: `position`, `username`, `name`, `school`, `city`, `province`, `medal`, `class`, `po`. If the `po` column is missing, no participant will be considered a PO. Position can be empy for unofficial contestants.
- A folder with the cmsRankingWebServer or terry or quizms data (`./ranking/`, mandatory).
- A folder with all the faces (`./faces/username.jpg`). Missing faces are substituted with a placeholder.
- A folder with all the screenshots (`./screenshots/username/date.png` or `date.jxl`; PNG and JPEG XL can be mixed, and the accepted extensions are set by `SCREEN_EXTENSIONS` in `config.py`). If screenshots for a user are missing, a placeholder is used instead. If all screenshots are missing, you can auto-generate placeholders if files `./screenshots/background.png` and `./screenshots/filler.png` are given. Auto-generation requires several minutes and will not be performed if files matching pattern `20*00.0.png` are already found in `./screenshots/`. 

## Instructions

#### Install manim

Tested on Fedora 43 with [uv](https://docs.astral.sh/uv/). System packages needed: a LaTeX distribution (`latex`, `dvisvgm`, the `standalone` class), `ffmpeg`, ImageMagick (`convert`), `gcc`, and the cairo/pango development headers (`cairo-devel`, `pango-devel`; `libcairo2-dev`, `libpango1.0-dev` on Debian/Ubuntu). `asy` is only needed for auto-generated screenshot placeholders.

```
uv venv --python 3.13 .venv
uv pip install --python .venv/bin/python "manim==0.18.1" pillow-jxl-plugin
source .venv/bin/activate   # or prefix the commands below with .venv/bin/
```

`pillow-jxl-plugin` lets Pillow (and thus manim) read JPEG XL screenshots; `ranking.py` imports it at startup, so it is required even if you only use PNG.

Two version constraints matter:

- **Manim must be 0.18.x.** `confetti.py` passes custom keyword arguments to `Animation.__init__`, which manim 0.19+ rejects (`TypeError: ... unexpected keyword argument 'x_start'`).
- **Python 3.13, not the newest one.** Manim's dependencies lag behind the latest Python release, so an older interpreter is the safer choice.

`.venv/` is ignored by git (uv adds its own `.gitignore` inside it).

#### Quick test with placeholder data

To check that the whole pipeline works without real data:

```
python make_test_data.py   # creates ranking.csv, ranking/, faces/, screenshots/ (15 fake users)
./preprocess.py
manim render -ql ranking.py Gold      # ~30 s; same for Silver, Bronze, Mention
```

The script refuses to run if any of those paths already exists, so it will not overwrite real data. All generated paths are git-ignored; delete them (and `output/`, `media/`) when done.

**Known issue:** re-running `manim render` on a scene that was already rendered fails with `ValueError: operands could not be broadcast together` (manim's animation cache skips the timelapse, so the score text is stale). Delete `./media` before re-rendering.

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

In `config.py` you can specify many other properties, including the subdivision in groups for medals (property `GROUPS_ARRAY`). If groups is `None`, no group summary picture is presented. Otherwise, each group is `(columns, rows, scale)`, optionally followed by the number of contestants in the group (by default `columns * rows`). A recap page is shown after each group. Mentions should have exactly one group, but the size of the group does not have to correspond to the number of mentions.

#### Render overlays

A few handy overlays are also provided: `Countdown` and `NameMarker`. To render them:

```
manim render -qh -t ./overlays.py [overlay]
```

They will prompt for the needed input parameters.
