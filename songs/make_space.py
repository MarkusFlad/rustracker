#!/usr/bin/env python3
"""'Orbitalfunk' - original instrumental space synth-pop, 8-channel MOD (8CHN).

Channels: 1 lead, 2 lead echo, 3 arpeggio, 4 pad, 5 bass, 6 kick, 7 snare, 8 hi-hat/crash.
All samples are synthesised here; melody, chords and arrangement are original.
"""
import re, struct, sys
import numpy as np

PAL = 7093789.2
R = PAL / (2 * 428)          # playback rate at C-2
RD = PAL / (2 * 214)         # playback rate at C-3 (drums)
PERIODS = [856,808,762,720,678,640,604,570,538,508,480,453,
           428,404,381,360,339,320,302,285,269,254,240,226,
           214,202,190,180,170,160,151,143,135,127,120,113]
NCH = 8
rng = np.random.default_rng(1983)

def to8(x, peak=118):
    x = x / (np.max(np.abs(x)) + 1e-9) * peak
    return np.clip(np.round(x), -128, 127).astype(np.int8)

def even(n): return n + (n & 1)

C3, C4, C2 = 130.81, 261.63, 65.41

# ---------------------------------------------------------------- samples
# 1 lead: pulse (25 %) + saw, looped, tuned C-2 = C4
L = 792; t = np.arange(L); ncyc = 25
x = np.zeros(L)
for k in range(1, 15):
    x += (np.sin(np.pi * k * 0.25) / k) * np.sin(2 * np.pi * k * ncyc * t / L)        # pulse
    x += 0.45 / k * np.sin(2 * np.pi * k * ncyc * t / L + 0.3)                        # saw
lead = to8(x, 112)

# 2 arp pluck: square-ish, fast decay, C-2 = C4
n = int(0.22 * R); tt = np.arange(n) / R
x = np.zeros(n)
for k in range(1, 12, 2):
    x += np.exp(-tt * (14 + 3 * k)) * np.sin(2 * np.pi * C4 * k * tt) / k
x[:12] *= np.linspace(0, 1, 12)
arp = to8(x, 110); arp[:2] = 0

# 3/4 pads: chord in the loop (exact integer cycles), slow attack before the loop, C-2 = C3
def pad(third):
    N = 40
    Lp = even(int(round(N * R / C3)))
    A = even(int(0.42 * R))
    tt = np.arange(A + Lp)
    def saw(cyc, amp, nh=12, ph=0.0):
        return amp * sum(np.sin(2 * np.pi * k * cyc * tt / Lp + ph * k) / k ** 1.35 for k in range(1, nh))
    x = (saw(N, 1.0) + saw(N + 1, 0.7, ph=0.5) + saw(N * third, 0.75) + saw(N * 1.5, 0.7)
         + saw(N * 1.5 + 1, 0.45, ph=1.1) + 0.6 * np.sin(2 * np.pi * (N // 2) * tt / Lp))
    env = np.ones(A + Lp); ramp = np.linspace(0, 1, A); env[:A] = ramp * ramp * (3 - 2 * ramp)
    d = to8(x * env, 105)
    d[:2] = 0
    return d, A, Lp
pad_min, pad_min_ls, pad_min_ll = pad(1.2)
pad_maj, pad_maj_ls, pad_maj_ll = pad(1.25)

# 5 bass pluck: saw + pulse, brightness decays faster than level, C-2 = C2
n = int(0.26 * R); tt = np.arange(n) / R
x = np.zeros(n)
for k in range(1, 16):
    a = 1 / k + (0.6 / k if k % 2 else 0)
    x += a * np.exp(-tt * (6 + 2.2 * k)) * np.sin(2 * np.pi * C2 * k * tt)
x = np.tanh(2.2 * x / np.max(np.abs(x)))
x[:8] *= np.linspace(0, 1, 8)
bass = to8(x, 118); bass[:2] = 0

# drums at RD (played at C-3)
def env(n, d): return np.exp(-np.arange(n) / RD * d)
n = int(0.42 * RD); tt = np.arange(n) / RD
kick = np.sin(2 * np.pi * np.cumsum(48 + 140 * np.exp(-tt * 32)) / RD) * env(n, 7)
kick[:30] += rng.standard_normal(30) * 0.5
n = int(0.26 * RD); tt = np.arange(n) / RD
nz = rng.standard_normal(n); nz = nz - np.convolve(nz, np.ones(4) / 4, 'same')
snare = (0.9 * nz * (0.35 + 0.65 * env(n, 30)) + 0.7 * np.sin(2 * np.pi * 185 * tt) * env(n, 26))
snare *= np.where(tt < 0.2, 1.0, np.maximum(0, 1 - (tt - 0.2) / 0.02))      # 80s gated reverb
n = int(0.05 * RD)
hat_c = np.diff(rng.standard_normal(n + 1)) * env(n, 70)
n = int(0.28 * RD)
hat_o = np.diff(rng.standard_normal(n + 1)) * env(n, 11)
n = int(1.6 * RD); tt = np.arange(n) / RD
crash = np.diff(rng.standard_normal(n + 1)) * env(n, 2.4)
for f in (3170, 4820, 6110, 7340):
    crash += 0.12 * np.sin(2 * np.pi * f * tt) * env(n, 3.5)
# zap: falling sine glide, a little "radio" noise
n = int(0.5 * RD); tt = np.arange(n) / RD
zap = np.sin(2 * np.pi * np.cumsum(2400 * np.exp(-tt * 7) + 120) / RD) * env(n, 5) + 0.05 * rng.standard_normal(n) * env(n, 9)
drums = [to8(d, p) for d, p in ((kick, 120), (snare, 112), (hat_c, 90), (hat_o, 90), (crash, 110), (zap, 100))]
for d in drums: d[:2] = 0

SAMPLES = [  # (name, data, volume, loopstart, looplen)
    ("lead", lead, 46, 0, len(lead)),
    ("arp pluck", arp, 34, 0, 0),
    ("pad minor", pad_min, 38, pad_min_ls, pad_min_ll),
    ("pad major", pad_maj, 38, pad_maj_ls, pad_maj_ll),
    ("bass pluck", bass, 58, 0, 0),
    ("kick", drums[0], 62, 0, 0),
    ("snare gated", drums[1], 50, 0, 0),
    ("hihat closed", drums[2], 26, 0, 0),
    ("hihat open", drums[3], 22, 0, 0),
    ("crash", drums[4], 40, 0, 0),
    ("zap", drums[5], 36, 0, 0),
]
LEAD, ARP, PADM, PADJ, BASS, KICK, SNARE, HATC, HATO, CRASH, ZAP = range(1, 12)

# ---------------------------------------------------------------- music (D minor)
KEYSIG = {'B': -1}
def parse(s):
    out = []
    for acc, letter, octs, dur in re.findall(r"([\^=_]?)([A-Ga-gz])([,']*)(\d*)", s.replace('|', ' ')):
        d = int(dur) if dur else 1
        if letter == 'z':
            out.append((None, d)); continue
        base = {'C': 0, 'D': 2, 'E': 4, 'F': 5, 'G': 7, 'A': 9, 'B': 11}[letter.upper()]
        midi = 60 + base + (12 if letter.islower() else 0) + 12 * octs.count("'") - 12 * octs.count(',')
        midi += {'^': 1, '_': -1, '=': 0}[acc] if acc else KEYSIG.get(letter.upper(), 0)
        out.append((midi, d))
    assert sum(d for _, d in out) == 32, (s, sum(d for _, d in out))
    return out

# chord: (pad root midi, minor?, arp root midi, bass root midi)
CH = {
    'Dm': (50, True, 62, 38), 'Bb': (46, False, 58, 34), 'F': (53, False, 65, 41),
    'C': (48, False, 60, 36), 'Gm': (43, True, 55, 31), 'A': (45, False, 57, 33),
}

class Pat:
    def __init__(self): self.c = [[None] * 64 for _ in range(NCH)]
    def set(self, ch, row, smp=0, midi_idx=None, eff=0, par=0):
        if 0 <= row < 64: self.c[ch][row] = (smp, midi_idx, eff, par)
    def eff(self, ch, row, eff, par):
        if not 0 <= row < 64: return
        cur = self.c[ch][row]
        if cur is None: self.c[ch][row] = (0, None, eff, par)
        elif cur[2] == 0: self.c[ch][row] = (cur[0], cur[1], eff, par)

def idx(midi, base):  # base = midi note that plays at C-1
    i = midi - base
    assert 0 <= i < 36, (midi, base)
    return i

def melody(p, s, vol=None):
    row = 0
    for m, d in parse(s):
        if m is None:
            p.set(0, row, 0, None, 0xC, 0)
        else:
            p.set(0, row, LEAD, idx(m, 48), 0xC if vol else 0, vol or 0)
            if d >= 3:
                for r in range(row + 4, row + 2 * d): p.eff(0, r, 0x4, 0x34)
        row += 2 * d

def echo(p, delay=3, vol=18):
    for r in range(64):
        c = p.c[0][r]
        if c is None: continue
        smp, n, e, par = c
        if e == 0xC and par == 0:
            p.set(1, r + delay, 0, None, 0xC, 0)
        elif n is not None:
            p.set(1, r + delay, smp, n, 0xC, vol)
        elif e == 0x4:
            p.eff(1, r + delay, 0x4, par)

def pads(p, chords, vols):
    for b, (name, v) in enumerate(zip(chords, vols)):
        root, minor, _, _ = CH[name]
        p.set(3, b * 16, PADM if minor else PADJ, idx(root, 24 + 12), 0xC, v)

def arps(p, chords, vol=34, bars=range(4)):
    for b in bars:
        root, minor, ar, _ = CH[chords[b]]
        third = 3 if minor else 4
        seq = [0, 7, 12, 7, third, 7, 12, third + 12 if ar + third + 12 <= 83 else 7]
        for r in range(16):
            accent = vol if r % 4 == 0 else int(vol * 0.7)
            p.set(2, b * 16 + r, ARP, idx(ar + seq[r % 8], 48), 0xC, accent)

def bassline(p, chords, bars=range(4), vol=56, style='octave'):
    for b in bars:
        root = CH[chords[b]][3]
        for e in range(8):
            if style == 'octave':
                n = root + (12 if e % 2 else 0)
            elif style == 'pulse':
                n = root
            else:  # 'long'
                if e % 4: continue
                n = root
            p.set(4, b * 16 + e * 2, BASS, idx(n, 24), 0xC, vol if e % 2 == 0 else vol - 10)

def drums(p, bars=range(4), kick=True, snare=True, hats=True, crash_first=False, vol=1.0):
    for b in bars:
        o = b * 16
        if kick:
            for r in (0, 4, 8, 12): p.set(5, o + r, KICK, 24, 0xC, int(62 * vol))
        if snare:
            for r in (4, 12): p.set(6, o + r, SNARE, 24, 0xC, int(50 * vol))
        if hats:
            for r in range(16):
                if r % 4 == 2: p.set(7, o + r, HATO, 24, 0xC, int(24 * vol))
                else: p.set(7, o + r, HATC, 24, 0xC, int((26 if r % 2 == 0 else 14) * vol))
    if crash_first: p.set(7, bars[0] * 16, CRASH, 24, 0xC, 44)

def roll(p, bar, start=20, end=56):
    for r in range(16):
        p.set(6, bar * 16 + r, SNARE, 24, 0xC, int(start + (end - start) * r / 15))

V = ['Dm', 'Bb', 'F', 'C']
CA = ['Bb', 'C', 'Dm', 'Dm']
CB = ['Gm', 'Bb', 'C', 'A']
BR = ['Gm', 'Dm', 'Bb', 'A']
pats = []

# 0 intro 1: zap, pad swelling, arp from bar 3
p = Pat(); pads(p, V, [14, 24, 34, 42]); arps(p, V, vol=22, bars=[2, 3])
p.set(2, 0, ZAP, 24, 0xC, 30)
p.eff(4, 0, 0xF, 124); p.eff(5, 0, 0xF, 6)
pats.append(p)
# 1 intro 2: bass + kick join, snare roll at the end
p = Pat(); pads(p, V, [42] * 4); arps(p, V, vol=30); bassline(p, V, style='pulse', vol=44)
drums(p, bars=range(3), snare=False, hats=False); roll(p, 3, 14, 50)
pats.append(p)
# 2/3 verse
for mel in ("z2 A2 d2 c A | B4 A2 G2 | A2 c2 f2 e c | e3 d c4",
            "z2 A2 d2 f2 | g3 f d2 B2 | c2 A2 c2 f2 | e6 z2"):
    p = Pat(); melody(p, mel); echo(p); pads(p, V, [34] * 4); arps(p, V, vol=28)
    bassline(p, V); drums(p, crash_first=(len(pats) == 2))
    pats.append(p)
# 4/5 chorus
for mel, ch in (("f4 d2 f2 | g4 e2 g2 | a3 g f2 e2 | d6 z2", CA),
                ("d4 B2 d2 | f4 d2 f2 | g3 a g2 e2 | ^c4 e4", CB)):
    p = Pat(); melody(p, mel, vol=56); echo(p, vol=22); pads(p, ch, [44] * 4); arps(p, ch, vol=32)
    bassline(p, ch, vol=60); drums(p, crash_first=(ch is CA))
    pats.append(p)
# 6 break: no drums but hats, long lead notes with echo
p = Pat(); melody(p, "G4 B4 | A8 | d4 c2 B2 | ^c8", vol=44); echo(p, delay=6, vol=20)
pads(p, BR, [40] * 4); bassline(p, BR, style='long', vol=46)
for b in range(4):
    for r in range(0, 16, 2): p.set(7, b * 16 + r, HATC, 24, 0xC, 14 if r % 4 else 20)
p.set(2, 0, ZAP, 24, 0xC, 26)
pats.append(p)
# 7 build: kick returns, snare roll into the chorus
p = Pat(); melody(p, "g4 f2 d2 | f4 e2 d2 | f4 g2 f2 | e8", vol=50); echo(p, delay=6, vol=20)
pads(p, BR, [44] * 4); arps(p, BR, vol=26, bars=[2, 3]); bassline(p, BR, style='pulse', vol=50)
drums(p, bars=range(2), snare=False, hats=False); roll(p, 2, 12, 34); roll(p, 3, 36, 60)
pats.append(p)
# 8 outro: verse chords, thinning out
p = Pat(); pads(p, V, [40, 34, 26, 20]); arps(p, V, vol=24); bassline(p, V, bars=range(2), style='pulse', vol=44)
drums(p, bars=range(2), snare=False, vol=0.8)
pats.append(p)
# 9 last chord: Dm pad fading, then end
p = Pat(); p.set(3, 0, PADM, idx(50, 36), 0xC, 40)
for r in range(1, 40): p.eff(3, r, 0xA, 0x01)
p.set(2, 0, ARP, idx(62, 48), 0xC, 30); p.set(2, 4, ARP, idx(69, 48), 0xC, 22); p.set(2, 8, ARP, idx(74, 48), 0xC, 16)
p.set(7, 0, CRASH, 24, 0xC, 30)
p.eff(3, 40, 0xD, 0)
pats.append(p)

ORDER = [0, 1, 2, 3, 4, 5, 2, 3, 4, 5, 6, 7, 4, 5, 8, 9]

def write_mod(path):
    out = bytearray(b"Orbitalfunk".ljust(20, b"\0"))
    smps = SAMPLES + [("", np.zeros(0, np.int8), 0, 0, 0)] * (31 - len(SAMPLES))
    fixed = []
    for name, data, vol, ls, ll in smps:
        if len(data) % 2: data = np.append(data, np.int8(0))
        assert ls % 2 == 0 and ll % 2 == 0 and ls + ll <= len(data), name
        out += name.encode()[:22].ljust(22, b"\0")
        out += struct.pack(">HBBHH", len(data) // 2, 0, vol, ls // 2, (ll // 2) if ll else 1)
        fixed.append(data)
    out += bytes([len(ORDER), 127]) + bytes(ORDER + [0] * (128 - len(ORDER))) + b"8CHN"
    for p in pats:
        for row in range(64):
            for ch in range(NCH):
                smp, i, eff, par = p.c[ch][row] or (0, None, 0, 0)
                per = PERIODS[i] if i is not None else 0
                out += bytes([(smp & 0xF0) | (per >> 8), per & 0xFF, ((smp & 0x0F) << 4) | eff, par])
    for data in fixed: out += data.tobytes()
    open(path, "wb").write(out)
    print("bytes", len(out), "patterns", len(pats), "orders", len(ORDER))

if __name__ == "__main__":
    write_mod(sys.argv[1])
