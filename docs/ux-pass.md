# Dock and notch feel pass (2026-11-16)

Compared with Bloom's open and close behaviour (ideas only). Exe 1,237,504 B before, 1,244,160 B after (+6,656 B).

## What changed

**Notch**
- **Leave grace.** Before, an open notch started closing the moment the pointer slipped past its edge. Now it waits 240 ms (shorter with Fast animations) and stays open if the pointer comes back. Nothing is drawn while it waits; it is one deadline. A pointer that only brushed the pill and never opened it still closes at once.
- **Song peek.** When a new song starts and the notch is closed, it opens by itself on the music page for about 3 seconds and then closes, unless the pointer is on it. The first song after ROQA starts does not peek. A full-screen app is never covered. No new setting.
- **Alerts don't fight the grace.** An alert or peek that opens the notch cancels a pending leave close, so it is not cut short.

**Dock auto-hide**
- **Edge strip moved off the taskbar.** The 2 px reveal strip sat on the screen's bottom edge, on top of a visible Windows 11 taskbar, so it ate clicks on Start and taskbar icons. It now runs along the bottom of the work area: on the screen edge when the taskbar auto-hides, and just above the taskbar when it shows.
- **Faster reveal.** Resting on the strip shows the dock after 90 ms, down from 250 ms. That feels instant, and a pointer sweeping along the edge still doesn't flash it.
- **Graceful leave.** Leaving a revealed dock now slides it away 400 ms later. Before, it could take up to 1.6 s because it waited for the 0.8 s recheck twice.
- **Dragging a window reacts at once.** Dropping a window onto or off the dock now updates auto-hide right away (one Windows "move ended" event). The 2 s recheck still runs only while a window covers the dock.

**Widgets**
- **New tiles: Processor and Network.** Processor shows the percent in use. Network shows download speed and, under it, upload speed.
- **Live only while seen.** The Memory, Disk, Processor and Network tiles are read and redrawn once a second, but only while an open notch shows the Widgets page with one of them. When the notch closes or the page changes, sampling stops and the readings are dropped. No thread and no timer at idle.

**Device alerts**
- These now hold for 4 s instead of 8 s and no longer show over a full-screen app (prayer alerts still do). The trailing full stop is gone from the text, to match the brand voice.

## Checked and left as is
- Re-entry while closing already reopens directly from where the motion is. Hover intent (an instant swell, then opening after 320 ms) already filters fly-bys. The three curves and reduced motion are unchanged.
- The dock's slide (180 ms settle) already reverses smoothly if it is shown again while leaving.

## Open
- `Cargo.toml` gained two feature flags of the existing `windows` crate (IpHelper, Ndis) for the network reading. There is no new dependency and `Cargo.lock` is unchanged, but this is outside `src/`.
- Maximizing a window with its button, without dragging, still raises no event ROQA hears. If the dock is showing, it only hides on the next focus change. Catching it would mean listening to every window move, which costs too much.
- The song peek has no off switch. If it feels busy, add a setting or limit it to the Media tab being last used.
- The Network tile counts physical adapters only, so VPN traffic is counted once, through the adapter it travels over.
