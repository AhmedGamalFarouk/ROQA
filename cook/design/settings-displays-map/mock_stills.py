from pathlib import Path
import sys

from PIL import Image


HERE = Path(__file__).resolve().parent
sys.dont_write_bytecode = True
sys.path.insert(0, str(HERE.parent / 'settings-redesign'))
from mock_stills import CLASSIC, LIGHT, Still


DISPLAYS = (
    (1, (0, 0, 2560, 1440), 125, True, False),
    (2, (-1920, 360, 1920, 1080), 100, False, True),
)


def heading(window, title, top):
    window.text(window.left, top + 11, title, 15, 600, mid=True)


def panel(window, top, height):
    window.rect((window.left, top, window.right, top + height), 'panel', 16)


def group(window, title, top, rows):
    heading(window, title, top)
    height = 8 + sum(52 if caption else 42 for _, caption, _ in rows)
    top += 28
    panel(window, top, height)
    row_top = top + 4
    for label, caption, on in rows:
        middle = row_top + (26 if caption else 21)
        window.text(window.left + 16, middle - (8 if caption else 0), label, 13, 600, mid=True)
        if caption:
            window.text(window.left + 16, middle + 10, caption, 12, mid=True, color='caption')
        window.switch(window.right - 16, middle, on)
        row_top += 52 if caption else 42
    return top + height + 16


def identity(window, left, center_y, number, primary, tile):
    label = str(number) if tile else f'Display {number}'
    size = 26 if tile else 13
    label_width = window.measure(label, size, 600)
    window.text(left, center_y, label, size, 600, mid=True)
    if primary:
        window.text(left + label_width + 8, center_y + (3 if tile else 0), 'Primary', 12,
                    color='caption', mid=True)


def metadata(rect, scale):
    return f'{rect[2]:,} × {rect[3]:,} · {scale}%'


def state(window, right, mid, on):
    window.text(right - 50, mid, 'On' if on else 'Off', 12, 600, align='r', mid=True)
    window.switch(right, mid, on)


def display_map(window, top):
    scale = min((window.right - window.left - 32) / 4480, 160 / 1440)
    left, inner_top = (window.left + window.right - 4480 * scale) / 2, top + 12
    height = 1440 * scale + 24
    panel(window, top, height)
    for number, rect, dpi, primary, on in DISPLAYS:
        left_screen, top_screen, width, depth = rect
        tile_x, tile_y = left + (left_screen + 1920) * scale, inner_top + top_screen * scale
        tile_w, tile_h = width * scale, depth * scale
        window.rect((tile_x + 4, tile_y + 4, tile_x + tile_w - 4, tile_y + tile_h - 4),
                    'selected' if on else 'control', 8)
        middle_x, middle_y = tile_x + tile_w / 2, tile_y + tile_h / 2
        label_width = window.measure(str(number), 26, 600)
        if primary:
            label_width += 8 + window.measure('Primary', 12)
        identity(window, middle_x - label_width / 2, middle_y - 23, number, primary, True)
        window.text(middle_x, middle_y + 2, metadata(rect, dpi), 12,
                    color='icon' if on else 'caption', mid=True, align='m')
        state_width = window.measure('On' if on else 'Off', 12, 600) + 50
        state(window, middle_x + state_width / 2, middle_y + 26, on)
    return top + height + 16


def display_list(window, top):
    height = 8 + 52 * len(DISPLAYS)
    panel(window, top, height)
    for index, (number, rect, dpi, primary, on) in enumerate(DISPLAYS):
        mid = top + 30 + index * 52
        if number == 2:
            window.rect((window.left + 4, mid - 24, window.right - 4, mid + 24), 'focused', 12)
            window.rect((window.left + 10, mid - 10, window.left + 13, mid + 10), 'ink', 1.5)
        identity(window, window.left + 22, mid - 9, number, primary, False)
        window.text(window.left + 22, mid + 10, metadata(rect, dpi), 12, color='caption', mid=True)
        state(window, window.right - 16, mid, on)
    return top + height + 16


def compact_frame(window):
    window.left, window.right = 12, window.width - 26
    window.bottom = window.height - 82
    window.rect((0, 0, window.width, 52), 'rail')
    window.rect((12, 8, window.width - 12, 44), 'control', 18)
    window.eyes(22, 20, small=True)
    window.text(47, 26, 'General', 18, 600, mid=True)
    window.chevron(window.width - 32, 26)
    window.rect((0, 52, window.width, window.bottom), 'body')
    window.text(window.left, 60, 'Where ROQA shows up.', 13, color='caption')
    window.rect((0, window.bottom, window.width, window.height), 'rail')
    window.text(window.left, window.bottom + 19, 'Keep at least one monitor on.', 12, mid=True)
    window.rect((window.right - 84, window.height - 42, window.right, window.height - 10), 'control', 16)
    window.text(window.right - 42, window.height - 26, 'Close', 13, 600, mid=True, align='m')


def render(width, height, palette, dpi, fallback, name):
    window = Still(width, height, 'General', 'Where ROQA shows up.', palette, dpi=dpi)
    if fallback:
        compact_frame(window)
    original = window.canvas.copy()
    top = group(window, 'Show ROQA', window.top + 4,
                (('Show the dock', '', True), ('Show the notch', '', True)))
    heading(window, 'Displays', top)
    window.text(window.left, top + 34, 'Choose where ROQA appears.', 12, color='caption', mid=True)
    top = (display_list if fallback else display_map)(window, top + 48)
    top = group(window, 'Behaviour', top, (
        ('Start with Windows', '', True),
        ('Hide from screen sharing', 'Skip recordings and shared screens.', False),
        ('Hide in full screen', '', True),
        ('Reminder sounds', 'For timers and prayer reminders.', True),
    ))
    footer_px = window.pt(0, window.bottom)[1]
    window.canvas.paste(original.crop((0, footer_px, original.width, original.height)), (0, footer_px))
    view = window.bottom - window.top
    whole = top - 16 + 12 - window.top
    if whole > view:
        bar_top, track = window.top + 4, view - 8
        thumb = max(28, track * view / whole)
        window.rect((window.right + 4, bar_top, window.right + 10, bar_top + track), 'track', 3)
        window.rect((window.right + 4, bar_top, window.right + 10, bar_top + thumb), 'thumb', 3)
    size = round(dpi * width), round(dpi * (height + 32))
    window.canvas.resize(size, Image.Resampling.LANCZOS).save(HERE / name, optimize=True)


if __name__ == '__main__':
    render(880, 680, CLASSIC, 1, False, 'classic-map.png')
    render(320, 472, LIGHT, 1.25, True, 'light-compact.png')
