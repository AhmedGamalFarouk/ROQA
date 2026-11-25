from functools import cache
from math import cos, pi, sin
from os import environ
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


HERE = Path(__file__).resolve().parent
FONT = Path(environ['WINDIR']) / 'Fonts' / 'SegUIVar.ttf'
CLASSIC = dict(body='#000000', rail='#0E0E10', panel='#141416', control='#26262A',
               selected='#3A3A40', focused='#424249', ink='#FFFFFF', caption='#9A9AA3',
               icon='#DCE1E8', lamp='#FFB454', knob='#E5E7EC', on_knob='#14161A',
               track='#19191C', thumb='#797981', chrome='#1B1B1D')
LIGHT = dict(body='#ECEEF2', rail='#E1E4EA', panel='#F7F8FA', control='#DEE1E7',
             selected='#C6CAD1', focused='#CBD0D9', ink='#14161A', caption='#50545F',
             icon='#333843', lamp='#9A5200', knob='#F7F8FA', on_knob='#FFFFFF',
             track='#DADDDF', thumb='#6E7380', chrome='#F5F6F9')
ROUTES = ('General', 'Appearance', 'Dock', 'Notch', 'Timers & water', 'Weather',
          'Prayer', 'Limits & agents', 'Hotkeys', 'About')


@cache
def face(scale, size, weight, optical):
    font = ImageFont.truetype(str(FONT), round(size * scale))
    font.set_variation_by_axes([weight, optical])
    return font


class Still:
    def __init__(self, width, height, title, subtitle, palette, dpi=1, tabs=()):
        self.width, self.height, self.dpi = width, height, dpi
        self.scale, self.ink = dpi * 2, palette
        self.canvas = Image.new('RGB', (self.px(width), self.px(height + 32)), palette['body'])
        self.draw = ImageDraw.Draw(self.canvas)
        self.compact = width < 720 or height < 584
        self.left = 16 if self.compact else 208
        self.right = width - (16 if self.compact else 24) - 14
        self.top = (86 + 38 * bool(tabs)) if self.compact else (84 + 44 * bool(tabs))
        self.bottom = height - 52
        self.rect((0, -32, width, 0), 'chrome')
        self.eyes(12, -22, small=True)
        self.text(33, -16, 'ROQA settings', 12, mid=True)
        self.line(width - 27, -16, [(0, -4), (8, 4)], 1)
        self.line(width - 27, -16, [(8, -4), (0, 4)], 1)
        if self.compact:
            self.rect((0, 0, width, 52), 'rail')
            self.rect((16, 8, width - 16, 44), 'control', 18)
            self.eyes(26, 20, small=True)
            self.text(51, 26, title, 18, 600, mid=True)
            self.chevron(width - 36, 26)
            self.text(self.left, 60, subtitle, 13, color='caption')
        else:
            self.rect((0, 0, 184, height), 'rail')
            self.eyes(22, 22)
            self.text(54, 29, 'roqa', 18, 600, mid=True)
            for index, route in enumerate(ROUTES):
                top = 64 + 44 * index
                if route == title:
                    self.rect((12, top, 172, top + 40), 'selected', 20)
                    self.rect((154, top + 17, 160, top + 23), 'lamp', 3)
                self.glyph(index, 24, top + 10)
                self.text(54, top + 20, route, 13, 600 if route == title else 400,
                          color='ink' if route == title else 'icon', mid=True, max_width=94)
            self.text(self.left, 22, title, 30, 600, optical=32)
            self.text(self.left, 60, subtitle, 13, color='caption')
        if tabs:
            x, top = self.left, 86 if self.compact else 84
            for label, active in tabs:
                width = self.measure(label, 13, 600) + 28 + 12 * active
                self.rect((x, top, x + width, top + 32), 'selected' if active else 'rail', 16)
                self.text(x + 14, top + 16, label, 13, 600, mid=True)
                if active:
                    self.rect((x + width - 16, top + 13, x + width - 10, top + 19), 'lamp', 3)
                x += width + 6
        self.rect((0 if self.compact else 184, self.bottom, self.width, height), 'rail')
        self.text(self.left, self.bottom + 26, 'Changes apply as you go.', 12, color='caption', mid=True)
        self.rect((self.right - 84, self.bottom + 10, self.right, self.bottom + 42), 'control', 16)
        self.text(self.right - 42, self.bottom + 26, 'Close', 13, 600, mid=True, align='m')

    def px(self, dip):
        return round(dip * self.scale)

    def pt(self, x, y):
        return self.px(x), self.px(y + 32)

    def rect(self, box, color, radius=0):
        left, top, right, bottom = box
        points = (*self.pt(left, top), *self.pt(right, bottom))
        if radius:
            self.draw.rounded_rectangle(points, radius=self.px(radius), fill=self.ink[color])
        else:
            self.draw.rectangle(points, fill=self.ink[color])

    def font(self, size, weight, optical=14):
        return face(self.scale, size, weight, optical)

    def measure(self, text, size, weight=400):
        return self.font(size, weight).getlength(text) / self.scale

    def text(self, x, y, text, size, weight=400, color='ink', mid=False,
             align='l', optical=14, max_width=None):
        font = self.font(size, weight, optical)
        if max_width is not None:
            assert font.getlength(text) <= self.px(max_width), text
        self.draw.text(self.pt(x, y), text, fill=self.ink[color], font=font,
                       anchor=align + ('m' if mid else 't'))

    def line(self, x, y, points, stroke=1.75, color='icon'):
        pixels = [self.pt(x + dx, y + dy) for dx, dy in points]
        width = self.px(stroke)
        self.draw.line(pixels, fill=self.ink[color], width=width, joint='curve')
        for point_x, point_y in (pixels[0], pixels[-1]):
            delta = (width - 1) / 2
            self.draw.ellipse((point_x - delta, point_y - delta, point_x + delta, point_y + delta),
                              fill=self.ink[color])

    def eyes(self, x, y, small=False):
        width, height, gap = (4, 10, 4) if small else (7, 14, 8)
        for left in (x, x + width + gap):
            self.rect((left, y, left + width, y + height), 'lamp', width / 2)

    def chevron(self, x, y):
        self.line(x - 4, y - 2, [(0, 0), (4, 4), (8, 0)], 1.5)

    def glyph(self, index, x, y):
        def loop(cx, cy, radius, start=0, sweep=2 * pi):
            return [(cx + cos(start + step * sweep / 32) * radius,
                     cy + sin(start + step * sweep / 32) * radius) for step in range(33)]

        paths = (
            [[(2, 9), (10, 2), (18, 9)], [(4, 8), (4, 17), (8, 17), (8, 13),
              (12, 13), (12, 17), (16, 17), (16, 8)]],
            [[(2, 5), (6, 5)], [(10, 5), (18, 5)], loop(8, 5, 2), [(2, 14), (11, 14)],
             loop(13, 14, 2), [(15, 14), (18, 14)]],
            [[(2, 14), (3, 16), (17, 16), (18, 14), (17, 12), (3, 12), (2, 14)],
             [(6, 8), (6, 7)], [(10, 8), (10, 7)], [(14, 8), (14, 7)]],
            [[(2, 5), (4, 5), (5, 6), (5, 9), (7, 11), (13, 11), (15, 9), (15, 6), (16, 5), (18, 5)]],
            [loop(10, 11, 6.5), [(8, 2), (12, 2)], [(10, 5), (10, 2)], [(10, 8), (10, 11), (13, 13)]],
            [loop(8, 7, 3.5, pi, 1.35 * pi), [(2, 7), (3, 7)], [(12, 3), (13, 2)],
             [(8, 1), (8, 2)], [(5, 16), (4, 14), (5, 12), (8, 11), (10, 12), (12, 9),
              (15, 10), (16, 12), (18, 13), (18, 15), (16, 17), (6, 17), (5, 16)]],
            [[(13, 2), (9, 3), (6, 6), (5, 10), (7, 14), (11, 17), (15, 16), (18, 12),
              (14, 12), (11, 10), (10, 7), (11, 4), (13, 2)]],
            [[(3, 16), (17, 16)], [(5, 13), (5, 10)], [(10, 13), (10, 6)], [(15, 13), (15, 3)]],
            [[(3, 5), (17, 5), (18, 6), (18, 14), (17, 15), (3, 15), (2, 14), (2, 6), (3, 5)],
             [(5, 8), (5.4, 8)], [(9, 8), (9.4, 8)], [(13, 8), (13.4, 8)], [(7, 12), (13, 12)]],
            [loop(10, 10, 7), [(10, 9), (10, 14)], [(10, 6), (10.1, 6)]],
        )
        for path in paths[index]:
            self.line(x, y, path)

    def switch(self, right, mid, on):
        self.rect((right - 42, mid - 12, right, mid + 12), 'lamp' if on else 'selected', 12)
        knob = right - 12 if on else right - 30
        self.rect((knob - 8, mid - 8, knob + 8, mid + 8), 'on_knob' if on else 'knob', 8)

    def groups(self, groups, at_end=False, focus=None):
        whole = sum(36 + sum(52 if row[1] else 42 for row in rows) for _, _, rows in groups)
        whole += 4 + 12 + 16 * (len(groups) - 1)
        view = self.bottom - self.top
        offset = max(0, whole - view) if at_end else 0
        previous = self.canvas.copy()
        y = self.top + 4 - offset
        anchors = {}
        for heading, hint, rows in groups:
            self.text(self.left, y + 11, heading, 15, 600, mid=True)
            if hint:
                self.text(self.right, y + 11, hint, 12, color='caption', mid=True, align='r')
            y += 28
            self.rect((self.left, y, self.right, y + 8 + sum(52 if row[1] else 42 for row in rows)),
                      'panel', 16)
            y += 4
            for label, caption, kind, value, width in rows:
                row_height = 52 if caption else 42
                mid = y + row_height / 2
                right, left = self.right - 16, self.right - 16 - width
                self.text(self.left + 16, mid - (8 if caption else 0), label, 13, 600,
                          mid=True, max_width=left - self.left - 32)
                if caption:
                    self.text(self.left + 16, mid + 10, caption, 12, color='caption', mid=True,
                              max_width=left - self.left - 32)
                if kind == 'switch':
                    self.switch(right, mid, value)
                else:
                    self.rect((left, mid - 16, right, mid + 16), 'focused' if value == focus else 'control', 16)
                    anchors[label] = left, mid + 16, right
                    if value == focus:
                        self.rect((left + 6, mid - 10, left + 9, mid + 10), 'ink', 1.5)
                    if kind == 'pick':
                        self.text(left + 14, mid, value, 13, mid=True, max_width=width - 42)
                        self.chevron(right - 18, mid)
                    elif kind == 'edit':
                        self.text(right - 14, mid, value, 13, mid=True, align='r')
                    else:
                        self.text((left + right) / 2, mid, value, 13, 600, mid=True, align='m')
                y += row_height
            y += 20
        top_px, bottom_px = self.pt(0, self.top)[1], self.pt(0, self.bottom)[1]
        for start, end in [(0, top_px), (bottom_px, self.canvas.height)]:
            self.canvas.paste(previous.crop((0, start, self.canvas.width, end)), (0, start))
        if whole > view:
            track = view - 8
            thumb = max(28, track * view / whole)
            travel = (track - thumb) * offset / (whole - view)
            bar_top = self.top + 4
            self.rect((self.right + 4, bar_top, self.right + 10, bar_top + track), 'track', 3)
            self.rect((self.right + 4, bar_top + travel, self.right + 10, bar_top + travel + thumb), 'thumb', 3)
        assert y - 16 + 12 == self.top + whole - offset
        return anchors

    def pin_popup(self, anchor):
        _, top, right = anchor
        left, height = right - 324, 8 * 36 + 8
        self.rect((left, top + 4, right + 4, top + height + 8), 'body', 16)
        self.rect((left, top + 4, right, top + height + 4), 'control', 16)
        options = ('Settings', 'File Explorer', 'Windows Terminal', 'Visual Studio Code',
                   'Visual Studio Installer', 'Microsoft Store', 'Remote Desktop Connection', 'Notepad')
        for index, value in enumerate(options):
            row = top + 8 + 36 * index
            if index == 2:
                self.rect((left + 4, row, right - 16, row + 36), 'selected', 18)
                self.rect((right - 32, row + 15, right - 26, row + 21), 'lamp', 3)
            self.text(left + 16, row + 18, value, 13, mid=True, max_width=275)
        self.rect((right - 10, top + 10, right - 4, top + height - 2), 'track', 3)
        self.rect((right - 10, top + 10, right - 4, top + 128), 'thumb', 3)

    def save(self, name):
        size = round(self.dpi * self.width), round(self.dpi * (self.height + 32))
        self.canvas.resize(size, Image.Resampling.LANCZOS).save(HERE / name, optimize=True)


def switch(label, value=True, caption=''):
    return label, caption, 'switch', value, 42


def pick(label, value, width=216):
    return label, '', 'pick', value, width


def button(label, value, caption='', width=112):
    return label, caption, 'button', value, width


def main():
    general = (
        ('Show ROQA', '', [switch('Show the dock'), switch('Show the notch')]),
        ('Displays', '', [switch('Display 1 (primary)', caption='2,560 × 1,440 at 125%'),
                          switch('Display 2', caption='1,920 × 1,080 at 100%')]),
        ('Behaviour', '', [switch('Start with Windows'),
                         switch('Hide from screen sharing', False, 'Skip recordings and shared screens.'),
                         switch('Hide in full screen'),
                         switch('Reminder sounds', caption='For timers and prayer reminders.')]),
    )
    prayer = (
        ('Reminders', '', [switch('Prayer reminders'), pick('Prayer method', 'Egyptian General Authority', 260),
                           pick('Heads-up before the prayer', '5 minutes before'),
                           switch('Pause media at prayer time', False, 'Requires the Media tab. Never resumes.')]),
        ('Dock', '', [switch('Moon on the dock')]),
        ('Time adjustments', 'Whole minutes, −30 to 30',
         [(name + ' shift', '', 'edit', '0', 80) for name in ('Fajr', 'Dhuhr', 'Asr', 'Maghrib', 'Isha')]),
        ('Sound', '', [pick('Prayer sound', 'takbeer1.mp3', 260), button('Choose a sound file', 'Browse…'),
                       button('Play a sample', 'Test sound', 'Works with Reminder sounds off.')]),
    )
    apps = (
        ('Pinned apps', '', [pick('Pin to move', 'Windows Terminal', 260),
                             button('Move pin earlier', 'Move earlier'), button('Move pin later', 'Move later')]),
        ('Hidden apps', '', [pick('Hidden app', 'Calculator', 260), button('Show the selected app', 'Show in dock')]),
    )
    window = Still(880, 680, 'General', 'Where ROQA shows up.', CLASSIC)
    window.groups(general)
    window.save('general-classic.png')
    window = Still(880, 680, 'Prayer', 'Tune prayer times and reminders.', CLASSIC)
    window.groups(prayer)
    window.save('prayer-classic.png')
    window = Still(600, 472, 'Prayer', 'Tune prayer times and reminders.', LIGHT, dpi=1.25)
    window.groups(prayer, at_end=True, focus='Test sound')
    window.save('prayer-compact-light.png')
    window = Still(880, 680, 'Dock', 'Choose what stays close.', CLASSIC,
                   tabs=(('Layout', False), ('Buttons', False), ('Widgets', False), ('Apps', True)))
    anchors = window.groups(apps)
    window.pin_popup(anchors['Pin to move'])
    window.save('dock-apps-classic.png')


if __name__ == '__main__':
    main()
