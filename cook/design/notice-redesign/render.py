from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont


SCALE = 4
OUT = Path(__file__).parent
THEMES = {
    'classic': ('#000000', '#141416', '#FFFFFF', '#B7B7BE', '#FFB454', '#14161A', '#686868'),
    'light': ('#ECEEF2', '#DDE0E6', '#14161A', '#494D55', '#9A5200', '#FFFFFF', '#A2A5AB'),
}
FIXTURES = {
    'short': 'Mail: Your receipt is ready',
    'long': 'Calendar: Studio review moved to the east conference room. Bring the revised drawings, finish samples, and the latest supplier estimates.',
    'reminder': 'Bring the signed lease',
}


def scaled(items):
    return tuple(round(value * SCALE) for value in items)


def font(size, weight=400):
    face = ImageFont.truetype('C:/Windows/Fonts/SegUIVar.ttf', size * SCALE)
    face.set_variation_by_axes([weight, size])
    return face


def trim(copy, face, width):
    if face.getlength(copy) <= width * SCALE:
        return copy
    while face.getlength(copy + '…') > width * SCALE:
        copy = copy[:-1]
    return copy.rstrip() + '…'


def two_lines(copy, face, width):
    first, rest = copy, ''
    while face.getlength(first) > width * SCALE:
        first, _, word = first.rpartition(' ')
        rest = (word + ' ' + rest).strip()
    return (first, trim(rest, face, width)) if rest else (copy,)


def draw_still(theme, fixture):
    panel, fill, ink, muted, lamp, on_lamp, backdrop = THEMES[theme]
    image = Image.new('RGB', scaled((800, 220)), backdrop)
    mask = Image.new('L', image.size)
    shape = ImageDraw.Draw(mask)
    shape.rounded_rectangle(scaled((220, -40, 580, 166)), radius=40*SCALE, fill=255)
    shape.rectangle(scaled((206, 0, 594, 14)), fill=255)
    shape.ellipse(scaled((192, 0, 220, 28)), fill=0)
    shape.ellipse(scaled((580, 0, 608, 28)), fill=0)
    shape.rounded_rectangle(scaled((347, 150, 453, 181)), radius=8*SCALE, fill=255)
    shadow = Image.new('L', image.size)
    shadow.paste(mask, (0, 6*SCALE))
    shadow = shadow.filter(ImageFilter.GaussianBlur(11*SCALE)).point(
        lambda alpha: int(alpha * (0.50 if theme == 'classic' else 0.24)))
    image.paste('#000000', (0, 0), shadow)
    image.paste(panel, (0, 0), mask)
    draw = ImageDraw.Draw(image)

    def rect(bounds, radius, color):
        draw.rounded_rectangle(scaled(bounds), radius=round(radius*SCALE), fill=color)

    def text(copy, x, y, size, color=ink, weight=400, anchor='lm'):
        draw.text(scaled((x, y)), copy, font=font(size, weight), fill=color, anchor=anchor)

    def line(points, color, width=1.75):
        draw.line([scaled(point) for point in points], fill=color, width=round(width*SCALE), joint='curve')
        for x, y in (points[0], points[-1]):
            radius = width / 2
            draw.ellipse(scaled((x-radius, y-radius, x+radius, y+radius)), fill=color)

    def ring(bounds, color, width=1.75):
        draw.ellipse(scaled(bounds), outline=color, width=round(width*SCALE))

    text('Media', 236, 20, 13, weight=600)
    rect((540, 7, 564, 31), 12, fill)
    for y, toggle in [(16.5, 550), (21.5, 554)]:
        line([(547, y), (557, y)], ink, 1)
        ring((toggle-1.5, y-1.5, toggle+1.5, y+1.5), ink, 1)
    text('2:33', 284, 54, 30, weight=700)
    text('am', 347, 58, 16, weight=700)
    text('Monday, 23 November 2026', 335, 83, 13, muted, anchor='mm')
    text('13 Jumada II 1448 AH', 335, 102, 11, muted, anchor='mm')
    for x in (491, 516):
        rect((x, 47, x+9, 65), 4.5, lamp)
    text('Nothing playing', 508, 95, 11, muted, anchor='mm')

    for index in range(6):
        x = 360 + 16 * index
        if index == 0:
            rect((x-3.5, 170, x+3.5, 177), 2, muted)
            draw.polygon([scaled(point) for point in ((x-1, 171.5), (x+2, 173.5), (x-1, 175.5))], fill=panel)
            rect((x-3.5, 178, x+3.5, 179.5), 0.75, lamp)
        else:
            rect((x-2.5, 171, x+2.5, 176), 2.5, muted)

    rect((244, 120, 556, 156), 16, fill)
    if fixture == 'reminder':
        ring((257, 131, 271, 145), muted)
        line([(264, 134), (264, 138), (267, 139.5)], muted)
        text('Reminder', 280, 131, 12, weight=600)
        text(trim(FIXTURES[fixture], font(12), 164), 280, 145, 12, muted)
        rect((452, 126, 544, 150), 12, lamp)
        text('Snooze 10 min', 498, 138, 12, on_lamp, 600, 'mm')
    else:
        line([(257.5, 141), (259, 138), (259, 135), (259.7, 133), (261.4, 131.5),
              (264, 131), (266.6, 131.5), (268.3, 133), (269, 135), (269, 138),
              (270.5, 141), (257.5, 141)], muted)
        line([(262.5, 144), (265.5, 144)], muted)
        lines = two_lines(FIXTURES[fixture], font(13), 264)
        for y, copy in zip((138,) if len(lines) == 1 else (130, 146), lines):
            text(copy, 280, y, 13)

    image.resize((800, 220), Image.Resampling.LANCZOS).save(OUT / f'{theme}-{fixture}.png')


if __name__ == '__main__':
    for theme_name in THEMES:
        for fixture_name in FIXTURES:
            draw_still(theme_name, fixture_name)
