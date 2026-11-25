# Eject a USB drive from Places

One extra text row in the existing Places popup, under each connected USB drive. It exists for the moment after the "Drive connected" alert is gone. Borrow only LumaBar's idea (eject from the bar) and nothing else.

## Interaction

- Open Places as today. The popup lists the Recycle Bin and each drive, read on the existing `places::ask_places` worker. For each drive that is **eligible** (below) add one row directly after its drive row: **Eject E:** (letter only, no label, so it fits 250 DIP).
- Same 34 DIP text-only row, hover capsule, hit area, order, dismissal and owner (the clicked dock HWND, `anchored::Owner`) as every other Places row. No icon, badge, divider, accent, tooltip, submenu, confirm dialog or new motion. All five themes inherit the menu palette.
- **Empty:** no eligible drive means the popup is exactly today's popup. No disabled row, no hint, no "no drives" line.
- **Full:** one or several USB drives each get their own Eject row after their drive row. Nothing is ever sorted or moved.
- Click: close the menu through the normal selection flow, then run the eject on the existing eject worker (`system_os::eject_in_background`, one at a time). Results use the existing notice row (`power.announce`) and the existing strings.

## Eligible drive (decided on the Places worker, never the UI thread)

All of these, or no row:
1. Volume has a letter and a mounted file system (the empty-card-reader skip already in `list()` stays).
2. On a handle opened for query only: storage bus type is USB (`IOCTL_STORAGE_QUERY_PROPERTY`, `StorageDeviceProperty`). This includes USB hard disks that Windows calls "fixed" and excludes internal disks and network drives. Do not use `DRIVE_REMOVABLE` alone.
3. Its physical device (`IOCTL_STORAGE_GET_DEVICE_NUMBER`) carries no other volume in the same list. A stick with two lettered partitions gets no Eject row: ejecting one partition would pull the device under the other. Limitation: partitions with no letter are invisible to this check.

Each eligible row keeps an identity captured in the same read: the volume GUID path (`GetVolumeNameForVolumeMountPointW`), the volume serial (`GetVolumeInformationW`, already called, currently discarded) and the device number. The letter is display text only. The identity lives in memory with the popup's `places` rows; nothing is saved or polled.

## Identity check (feasible; this is the fail-closed part)

The drive letter must never be what the eject trusts. The eject takes the captured identity and:

1. Opens `\\?\Volume{GUID}` (no trailing backslash) read/write, never `\\.\E:`. Windows resolves the GUID to the volume once, at open. If that volume is gone or the GUID names another volume, the open fails and nothing is touched.
2. **On that same handle, before any lock:** `GetFinalPathNameByHandleW(VOLUME_NAME_GUID)` equals the captured GUID; `GetVolumeInformationByHandleW` serial equals the captured serial; `IOCTL_STORAGE_GET_DEVICE_NUMBER` equals the captured number; bus type is still USB. Any mismatch closes the handle and returns "changed".
3. Only then the existing sequence on that handle: lock (fails while a file is open, so nothing is lost), dismount, allow removal, eject. Close the handle.

A reused letter therefore changes nothing: the new volume has a different GUID and serial, so the open or the check fails and no lock or dismount is sent. The same stick plugged in again is the same volume and may be ejected.

## Result states

| State | Notice (existing row) |
| --- | --- |
| Click accepted | **Ejecting E:** (existing) |
| Lock refused, file open or any step refused | **E: still in use** (existing) |
| All steps accepted | **E: safe to remove** (existing) |
| Drive removed, replaced, letter reused, or any identity check failed | **That drive isn't there now** (new, 5 words). Nothing was locked or dismounted |
| Another eject running or its answer unread | **Still ejecting another drive** (existing, from the alert's Eject). The row stays; click again later |
| Menu closed or refreshed before the click, slot or owner changed | Silent. Never act on a stale row |

The only new copy is one string and one new answer value: the one-slot eject answer (`letter << 8 | 2` refused, `| 3` safe) gains `| 4` changed, and `ejected()` returns three cases instead of a bool.

## Rust handoff

Read `.agents/skills/rust-best-practices/SKILL.md`; follow `AGENTS.md` where they differ.
- `places::Place` gains `eject: Option<Volume>` (GUID string, serial, device number, letter). `list()` fills it. `dock_menu::Item` gains `Eject(usize)` (index into the same `places`); `open_places_menu` emits it after the matching `Item::Place`. Add it to the menu's existing exhaustive matches (`describes`, row text, `toggled_by` stays `Button::Places`).
- `system_os::eject` takes the identity, not a letter, and does steps 1 to 3 above. One `unsafe` block per call with a `SAFETY` note, handle closed on every path (reuse the existing `OwnedHandle` if it fits).
- **The arrival alert's Eject moves to the same identity check (owner decision 2026-11-24).** It currently passes only a letter (`native_window::drive_read`, `power::Extra::Eject(char)`), which has the race this design closes. The shared `eject` is identity-only. The alert captures the volume's identity (GUID, serial, device number) at arrival, on a worker, not the UI thread, and carries it with the letter. If that capture fails or the drive fails the eligibility rules, the alert shows no Eject button. One `eject` path, no letter-only caller left.
- Tests (no hardware): the answer slot round trip with the new value; identity-mismatch decision as a pure function; eligibility as a pure function over (bus type, device numbers); row insertion order; stale-row rejection. Real eject needs a second-monitor live check with a USB stick: open file blocks the lock, unplug before click, plug a different stick into the same letter, two sticks, one two-partition stick. `scripts\verify.ps1` after.
- Not allowed: UI-thread waits, a new persistent worker, polling, saving anything, a size ceiling raise without saying so (`host.rs` stays under its ceiling by keeping the logic in `places.rs` and `system_os.rs`).

## Evidence and limits

- Checked in code: `places::list`, `ask_places`, `open_places_menu`, `Item::Place` activation, `system_os::eject` and `eject_in_background`, `ejected()` handling in `host.rs`, `show_device`, `native_window::drive_read`. Existing `eject` opens `\\.\{letter}:` with no check at all; that is the risk this design removes.
- Stills (Classic only, static HTML rendered with Chrome, not Direct2D captures, no live app): [classic.png](places-usb-eject/classic.png) shows none, one and two sticks with hover on an Eject row; [states.png](places-usb-eject/states.png) shows the six result states as black capsules approximating the notice row. Other themes only recolour existing rows and are not drawn. Row widths and the notice pill size are approximate.
- Not verified: real USB bus-type answers for card readers, SD adapters and USB-to-SATA bridges; whether one volume's eject on a multi-partition device behaves as assumed (hence the single-volume rule); `GetVolumeInformationByHandleW` on a sick drive can stall, acceptable only because it runs on the worker. No independent critique round (no subagents in this thread).
- An unplugged-while-open and "data written but ejected" case is covered only by the lock refusing while files are open; ROQA never force-dismounts.

## Built as (implement thread)

- Identity check uses calls Microsoft documents for volume handles: open `\?\Volume{GUID}`, then on that handle `IOCTL_STORAGE_GET_DEVICE_NUMBER`, `IOCTL_STORAGE_QUERY_PROPERTY` (bus type USB) and `IOCTL_MOUNTDEV_QUERY_UNIQUE_ID` must equal what was captured. `GetFinalPathNameByHandleW` and `GetVolumeInformationByHandleW` are documented for file handles, not volume device handles, so they are not used; the volume serial is not part of the identity.
- Single-volume rule walks `FindFirstVolumeW`, so unlettered and folder-mounted siblings count; any unreadable volume rules the device out. Read again on the worker just before the lock.
- The arrival alert still ejects by letter (`system_os::eject`): it can hit a reused letter. Not extended to Places.
