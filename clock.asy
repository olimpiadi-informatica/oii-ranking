settings.tex = "pdflatex";

string to_string(int x) {
    return (x < 10 ? "0" : "") + string(x);
}

void clock(string date, int h, int m) {
    picture pic;
    label(pic, graphic("screenshots/background.png"), (0,0));
    layer(pic);
    path c = circle((0,0), 460);
    picture subpic;
    label(subpic, graphic("screenshots/filler.png"), (0,0));
    clip(subpic, c);
    add(pic, subpic);
    layer(pic);
    draw(pic, c, black+48);
    for (int i=0; i<60; ++i)
        dot(pic, 400*dir(6*i), black+16);
    for (int i=0; i<12; ++i) {
        real l = i%3 == 0 ? 64 : 32;
        pair p = dir(30*i);
        draw(pic, (400-l)*p -- 400*p, black+24);
    }
    path hl = (0,0) -- (-5,30) -- (0,60) -- (5,30) -- cycle;
    fill(pic, rotate(-30*h)*scale(4)*hl, black);
    path ml = (0,0) -- (-5,60) -- (0,90) -- (5,60) -- cycle;
    fill(pic, rotate(-6*m)*scale(4)*ml, black);
    fill(pic, circle((0,0), 12), black);
    fill(pic, circle((0,0), 6), white);
    shipout("screenshots/" + date + "T" + to_string(h) + ":" + to_string(m) + ":00.0", pic);
}

string[] words = split(stdin, " ");
string start = words[0];
string end = words[1];
words = split(start, "T");
string date = words[0];
words = split(words[1], ":");
int h = (int)words[0];
int m = (int)words[1];
words = split(end, "T");
words = split(words[1], ":");
int eh = (int)words[0];
int em = (int)words[1];

while (true) {
    clock(date, h, m);
    if (h == eh && m == em) break;
    ++m;
    if (m == 60) {
        ++h;
        m = 0;
    }
}
