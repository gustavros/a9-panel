# ATK A9 Ultimate HID protocol (through the 373B:11D9 receiver)

Source: ATK Hub v3.2.27, `static/index-F-MYsmWR.js` (static reading, nothing executed), checked against a real mouse.
Purpose: interoperability with a mouse you own.

## Channel
- Receiver `VID 0x373B`, `PID 0x11D9`. Interface `usage_page 0xFF02`, `usage 2`: report ID 8, 16-byte input and output reports (confirmed in the HID descriptor).
- Send: output report `[8, ...16 bytes]`. Reply: input report `[8, ...16 bytes]` with `byte 0 = command`.
- An idle mouse sleeps: the first reply can take 1 to 3 s. The app retries 3 times.

## Packet (16 bytes)
| byte | field |
|------|-------|
| 0 | command |
| 1 | status |
| 2-3 | EEPROM address (big endian) |
| 4 | number of data bytes (up to 10) |
| 5-14 | data |
| 15 | checksum = `85 - (8 + sum of bytes 0..14)` |

## Commands
| id | use |
|----|-----|
| 3 | mouse online |
| 4 | battery: level %, charging, voltage mV (u16 BE) |
| 7 / 8 | write / read EEPROM |
| 9 | factory reset |
| 14 / 15 | read / change config file (profile) |
| 16 | CID/MID (A9 Ultimate = 1,3) |
| 18 | mouse firmware version |
| 20 / 21 | simple receiver light (0 off, 1 connection/polling, 2 battery, 3 low battery) |
| 22 / 23 | long distance mode |
| 29 | receiver firmware version |
| 13 | firmware update mode: **not used** |

## EEPROM
Almost everything is stored as `value, 85 - value` pairs.

| address | content |
|---------|---------|
| 0 | polling (+0), number of DPI levels (+2), current level 0-based (+4), bhop (+6), button mode (+8) |
| 10 | lift-off distance: 1..11 = 0.7 to 1.7 mm (Ultimate models) |
| 12, 20, 28, 36 | DPI: 2 levels per block, 4 bytes each `[x, y, extra, crc]` |
| 44..68 | level colors (the A9 Ultimate has no DPI light) |
| 96 + 4·n | button n (0 left, 1 right, 2 middle, 3 back, 4 forward, 5 DPI): `[class, v1, v2, crc]` |
| 169 | debounce ms (+0), motion sync (+2), sleep time /10 s (+4), angle snapping (+6), ripple control (+8) |
| 181 | sensor sleep (+0), its time /10 s (+2), sensor mode 0/1/2 (+4) |
| 768 + 384·n | macro n (0..11) |

Polling codes: 125=8, 250=4, 500=2, 1000=1, 2000=16, 4000=32, 8000=64.

### DPI (PAW3950 Ultra)
- Up to 10000: step 10, `byte = dpi/10 - 1`, no flag.
- 10050 to 30000: step 50, `(dpi - 10050)/50`, flag 2.
- Above 30000: step 100, `(dpi/2 - 10050)/50`, flags 2|1 (doubled).
- `extra = (high_y << 6) | (high_x << 2) | flags_x | (flags_y << 4)`; `crc = 85 - (x + y + extra)`.

### Buttons
Class 0 disabled, 1 mouse (v1: 1 left, 2 right, 4 middle, 8 back, 16 forward), 2 DPI (1 cycle, 2 +, 3 −),
3 horizontal scroll (1 left, 2 right), 6 macro (v1 = slot, v2 = repeat count, 254 while held, 253 until pressed again), 11 wheel (1 up, 2 down).

### Macros
`[8, 2×8, 255×22]` + `[count]` + 5-byte events + `crc = 85 - sum(count + events)`, written in 10-byte chunks.
Event: `[state|type, code, 0, delay_ms u16 BE]`. State: 0x80 press, 0x40 release, 0 wheel.
Type: 000 modifier (code = mask Ctrl 1, Shift 2, Alt 4, Win 8), 001 key (HID code), 100 mouse button, 110 wheel (1 up, 255 down). At most 70 events.
