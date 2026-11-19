#!/usr/bin/env python3
"""Build 'November Static' as a 4-channel ProTracker MOD (M.K.) and render a preview WAV."""
import re, struct, sys
import numpy as np

PAL = 7093789.2
R = PAL / (2 * 428)          # playback rate at C-2 (~8287 Hz)
RD = PAL / (2 * 214)         # playback rate at C-3 (~16574 Hz)
PERIODS = [856,808,762,720,678,640,604,570,538,508,480,453,
           428,404,381,360,339,320,302,285,269,254,240,226,
           214,202,190,180,170,160,151,143,135,127,120,113]
rng = np.random.default_rng(7)

# ---------------------------------------------------------------- samples
def to8(x, peak=118):
    x = x / (np.max(np.abs(x)) + 1e-9) * peak
    return np.clip(np.round(x), -128, 127).astype(np.int8)

def harmonic_loop(f_nom, ncyc, weights, rate=R):
    L = int(round(ncyc * rate / f_nom))
    t = np.arange(L)
    x = np.zeros(L)
    for k, w in enumerate(weights, 1):
        x += w * np.sin(2 * np.pi * k * ncyc * t / L + rng.uniform(0, 6.28))
    return x, L

def formant_weights(f0, n, formants=((700, 130), (1220, 170), (2600, 250)), tilt=1.0):
    w = []
    for k in range(1, n + 1):
        f = k * f0
        g = sum(a * np.exp(-0.5 * ((f - fc) / bw) ** 2) for (fc, bw), a in zip(formants, (1.0, 0.6, 0.3)))
        w.append((0.25 / k ** tilt) + g)
    return w

C3 = 130.81
# 1 lead soft: breathy "ah"
x, L = harmonic_loop(C3, 25, formant_weights(C3, 28, tilt=1.2))
lead_soft = to8(x, 100)
# 2 lead harsh: raspy, driven, with looped noise grit
x, L = harmonic_loop(C3, 25, formant_weights(C3, 30, tilt=0.6))
x = np.tanh(3.0 * x / np.max(np.abs(x)))
x += 0.18 * rng.standard_normal(L)
lead_harsh = to8(x, 120)
# 3 distorted power chord (root+5th+octave, slight detune for width)
L = int(round(40 * R / C3)); t = np.arange(L)
def saw(cyc):
    return sum(np.sin(2 * np.pi * k * cyc * t / L) / k for k in range(1, 14))
x = saw(40) + saw(41) * 0.6 + 0.8 * saw(60) + 0.5 * saw(80)
x = np.tanh(6 * x / np.max(np.abs(x)))
dist = to8(x, 110)
# 4 clean pluck (non-looped, chorused)
n = 5200; t = np.arange(n) / R
x = np.zeros(n)
for k in range(1, 12):
    a = 1.0 / k ** 1.1; d = 3.5 + 1.6 * k
    for det in (1.0, 1.004):
        x += a * np.exp(-d * t) * np.sin(2 * np.pi * C3 * k * det * t)
x[:30] *= np.linspace(0, 1, 30)
pluck = to8(x, 115); pluck[:2] = 0
# 5 bass (looped, C-2 = C2)
x, L = harmonic_loop(65.41, 26, [1, .5, .35, .2, .12, .08, .05, .03])
bass = to8(np.tanh(1.8 * x / np.max(np.abs(x))), 115)
# drums at RD
def env(n, d): return np.exp(-np.arange(n) / RD * d)
n = int(0.35 * RD); tt = np.arange(n) / RD
kick = np.sin(2 * np.pi * np.cumsum(45 + 110 * np.exp(-tt * 28)) / RD) * env(n, 9)
kick[:40] += rng.standard_normal(40) * 0.4
n = int(0.22 * RD); tt = np.arange(n) / RD
nz = rng.standard_normal(n); nz = nz - np.convolve(nz, np.ones(3) / 3, 'same')
snare = 0.9 * nz * env(n, 16) + 0.6 * np.sin(2 * np.pi * 190 * tt) * env(n, 22)
n = int(0.06 * RD)
hat = np.diff(rng.standard_normal(n + 1)) * env(n, 60)
n = int(1.4 * RD)
crash = np.diff(rng.standard_normal(n + 1)) * env(n, 2.6)
kc = crash * 0.55; kc[:len(kick)] += kick
drums = [to8(kick), to8(snare), to8(hat, 90), to8(kc)]
for d in drums: d[:2] = 0

# (name, data, volume, loopstart, looplen) ; looplen 0 = no loop
SAMPLES = [
    ("lead soft", lead_soft, 40, 0, len(lead_soft)),
    ("lead harsh", lead_harsh, 64, 0, len(lead_harsh)),
    ("dist guitar", dist, 44, 0, len(dist)),
    ("clean pluck", pluck, 48, 0, 0),
    ("bass", bass, 52, 0, len(bass)),
    ("kick", drums[0], 64, 0, 0),
    ("snare", drums[1], 60, 0, 0),
    ("hihat", drums[2], 36, 0, 0),
    ("kick+crash", drums[3], 64, 0, 0),
]
LEAD_S, LEAD_H, DIST, PLUCK, BASS, KICK, SNARE, HAT, KC = range(1, 10)

# ---------------------------------------------------------------- music
KEYSIG = {'F': 1}  # E minor: F#
def parse_melody(s):
    """ABC-ish (L:1/8, treble-8) -> list of (sounding midi or None, eighths)."""
    out = []
    for tok in re.findall(r"([\^=_]?)([A-Ga-gz])([,']*)(\d*)", s.replace('|', ' ')):
        acc, letter, octs, dur = tok
        d = int(dur) if dur else 1
        if letter == 'z':
            out.append((None, d)); continue
        base = {'C': 0, 'D': 2, 'E': 4, 'F': 5, 'G': 7, 'A': 9, 'B': 11}[letter.upper()]
        midi = 60 + base + (12 if letter.islower() else 0)
        midi += 12 * octs.count("'") - 12 * octs.count(',')
        midi += {'^': 1, '_': -1, '=': 0}[acc] if acc else KEYSIG.get(letter.upper(), 0)
        out.append((midi - 12, d))   # treble-8 sounds an octave lower
    assert sum(d for _, d in out) == 32, (s, sum(d for _, d in out))
    return out

CH = {  # name: (guitar root midi, quality, bass midi)
    'Em': (52, 'm', 28), 'Em/G': (52, 'm', 31), 'C': (48, 'M', 36), 'B': (47, 'M', 35),
    'Am': (45, 'm', 33), 'E5': (40, '5', 28), 'G5': (43, '5', 31), 'Bb5': (46, '5', 34),
    'A5': (45, '5', 33), 'F5': (41, '5', 29),
}
ARP = {'m': [0, 7, 12, 15, 19, 15, 12, 7], 'M': [0, 7, 12, 16, 19, 16, 12, 7]}

class Pat:
    def __init__(self):
        self.c = [[None] * 64 for _ in range(4)]
    def set(self, ch, row, smp=0, idx=None, eff=0, par=0):
        self.c[ch][row] = (smp, idx, eff, par)
    def eff(self, ch, row, eff, par):
        if row >= 64: return
        cur = self.c[ch][row]
        if cur is None: self.c[ch][row] = (0, None, eff, par)
        elif cur[2] == 0: self.c[ch][row] = (cur[0], cur[1], eff, par)

def idx_lead(m):  return m - 36
def idx_gtr(m):   return m - 36
def idx_bass(m):  return m - 24

def put_melody(p, mel, smp, vol=None, vib=0x44, porta_last=False):
    row = 0
    for i, (m, d) in enumerate(mel):
        if m is None:
            p.set(0, row, 0, None, 0xC, 0)
        else:
            last = porta_last and i == len(mel) - 1
            if last:
                p.set(0, row, 0, idx_lead(m), 0x3, 0x10)
                for r in range(row + 1, row + 2 * d): p.eff(0, r, 0x3, 0)
            else:
                p.set(0, row, smp, idx_lead(m), 0xC if vol else 0, vol or 0)
                if d >= 4:
                    for r in range(row + 3, row + 2 * d): p.eff(0, r, 0x4, vib)
        row += 2 * d

def put_arps(p, chords, vol=None, bars=range(4)):
    for b in bars:
        root, q, _ = CH[chords[b]]
        for e, off in enumerate(ARP[q]):
            p.set(1, b * 16 + e * 2, PLUCK, idx_gtr(root + off), 0xC if vol else 0, vol or 0)

def put_power(p, chords, bars=range(4), mode='8ths'):
    for b in bars:
        name = chords[b]
        if isinstance(name, tuple):          # two chords, half bar each, held
            for h, nm in enumerate(name):
                p.set(1, b * 16 + h * 8, DIST, idx_gtr(CH[nm][0]), 0xC, 44)
            continue
        root = CH[name][0]
        if mode == 'held':
            p.set(1, b * 16, DIST, idx_gtr(root), 0xC, 40)
        else:
            for e in range(8):
                p.set(1, b * 16 + e * 2, DIST, idx_gtr(root), 0xC, 44 if e % 2 == 0 else 30)

def put_bass(p, chords, bars=range(4), every=4, vol=48):
    for b in bars:
        name = chords[b]
        names = name if isinstance(name, tuple) else (name,)
        for h, nm in enumerate(names):
            span = 16 // len(names)
            for r in range(0, span, every):
                p.set(2, b * 16 + h * span + r, BASS, idx_bass(CH[nm][2]), 0xC, vol)

def put_drums(p, bars, vol, crash_first=True, fill_last=False):
    seq = [KICK, HAT, SNARE, HAT, KICK, KICK, SNARE, HAT]
    for b in bars:
        for e, s in enumerate(seq):
            v = vol if s != HAT else min(vol, 40)
            if crash_first and b == bars[0] and e == 0: s, v = KC, 64
            p.set(3, b * 16 + e * 2, s, 24, 0xC, v)
        if fill_last and b == bars[-1]:
            for e in range(4, 8):
                p.set(3, b * 16 + e * 2, SNARE, 24, 0xC, vol - 12 + e * 4)

V_CH = ['Em', 'Em/G', 'C', 'B']
PRE_CH = ['Am', 'C', 'Am', 'B']
CHO_CH = ['E5', 'G5', 'Bb5', 'A5']

pats = []
# 0 Intro
p = Pat(); put_arps(p, V_CH)
for e in range(4, 8): p.set(3, 48 + e * 2, SNARE, 24, 0xC, 16 + e * 4)
p.eff(2, 0, 0xF, 116); p.eff(3, 0, 0xF, 6)
pats.append(p)
# 1/2 Verse A/B
for mel in ("z2 E E G2 E2|B,2 D2 E4|z2 E E G2 A2|B2 A2 F4",
            "z2 E E G2 E2|B,2 D E E4|z2 E E G2 A2|B2 A G F4"):
    p = Pat(); put_melody(p, parse_melody(mel), LEAD_S); put_arps(p, V_CH)
    put_bass(p, V_CH, every=8, vol=40); put_drums(p, range(4), 30, crash_first=False)
    pats.append(p)
# 3 Pre-chorus
p = Pat(); put_melody(p, parse_melody("z2 A A c2 B2|z2 G A B4|A A B c d2 B2|^A2 B6"), LEAD_S, vol=52)
put_arps(p, PRE_CH, vol=56, bars=range(3)); put_power(p, PRE_CH, bars=[3], mode='held')
put_bass(p, PRE_CH, every=4, vol=46); put_drums(p, range(4), 40, crash_first=False, fill_last=True)
pats.append(p)
# 4/5 Chorus A/B
for mel in ("e2 e2 g4|e2 e2 a4|z d d d =f2 e2|e2 a6", "e2 e2 g4|e2 e2 a4|z d d d =f2 e2|e2 b6"):
    p = Pat(); put_melody(p, parse_melody(mel), LEAD_H, vib=0x46); put_power(p, CHO_CH)
    put_bass(p, CHO_CH, every=2, vol=56); put_drums(p, range(4), 56)
    pats.append(p)
# 6 Bridge 1: feedback, spoken (no lead), then scream
BR1 = [('E5', 'Bb5'), ('E5', 'Bb5'), 'E5', 'Bb5']
p = Pat(); put_melody(p, parse_melody("z8|z8|z2 b b b2 a2|b8"), LEAD_H, vib=0x48)
put_power(p, BR1, bars=[0, 1]); put_power(p, BR1, bars=[2, 3])
put_bass(p, BR1, bars=[0, 1], every=8, vol=44); put_bass(p, BR1, bars=[2, 3], every=2, vol=56)
p.set(3, 0, KC, 24, 0xC, 64); p.set(3, 16, KC, 24, 0xC, 48); put_drums(p, [2, 3], 56)
pats.append(p)
# 7 Bridge 2: scream x2, slide F5 -> E5, voice slides f -> e
BR2 = ['E5', 'Bb5', 'E5', ('F5', 'E5')]
p = Pat(); put_melody(p, parse_melody("z2 b b b2 a2|b8|z2 b b b2 a2|=f4 e4"), LEAD_H, vib=0x48, porta_last=True)
put_power(p, BR2); put_bass(p, BR2, every=2, vol=56); put_drums(p, range(4), 58, fill_last=True)
pats.append(p)
# 8 Outro (whispered)
p = Pat(); put_melody(p, parse_melody("z2 E E G4|z2 E E D4|E E E G A2 G2|F2 ^D6"), LEAD_S, vol=22, vib=0x33)
put_arps(p, V_CH, vol=40); p.set(2, 0, 0, None, 0xC, 0); p.set(3, 0, 0, None, 0xC, 0)
pats.append(p)
# 9 Final bar: Em arp once, ring out, end
p = Pat(); p.set(0, 0, 0, None, 0xC, 0)
for e, off in enumerate([0, 7, 12, 15]):
    p.set(1, e * 4, PLUCK, idx_gtr(52 + off), 0xC, 36)
p.eff(1, 40, 0xD, 0)
pats.append(p)

ORDER = [0, 1, 2, 3, 4, 5, 1, 2, 3, 4, 5, 6, 7, 4, 5, 8, 9]

# ---------------------------------------------------------------- write MOD
def write_mod(path):
    out = bytearray()
    out += b"November Static".ljust(20, b"\0")
    smps = SAMPLES + [("", np.zeros(0, np.int8), 0, 0, 0)] * (31 - len(SAMPLES))
    for name, data, vol, ls, ll in smps:
        if len(data) % 2: data = np.append(data, np.int8(0))
        out += name.encode()[:22].ljust(22, b"\0")
        out += struct.pack(">HBBHH", len(data) // 2, 0, vol, ls // 2, (ll // 2) if ll else 1)
    out += bytes([len(ORDER), 127]) + bytes(ORDER + [0] * (128 - len(ORDER))) + b"M.K."
    for p in pats:
        for row in range(64):
            for ch in range(4):
                cell = p.c[ch][row] or (0, None, 0, 0)
                smp, idx, eff, par = cell
                per = PERIODS[idx] if idx is not None else 0
                if idx is not None: assert 0 <= idx < 36, (ch, row, idx)
                out += bytes([(smp & 0xF0) | (per >> 8), per & 0xFF, ((smp & 0x0F) << 4) | eff, par])
    for name, data, *_ in smps:
        if len(data) % 2: data = np.append(data, np.int8(0))
        out += data.tobytes()
    open(path, "wb").write(out)
    return out

# ---------------------------------------------------------------- read back + render (verifies the file)
def render(path, sr=44100):
    b = open(path, "rb").read()
    sm = []
    off = 20
    for i in range(31):
        name, ln, ft, vol, ls, ll = struct.unpack(">22sHBBHH", b[off:off + 30]); off += 30
        sm.append(dict(len=ln * 2, vol=vol, ls=ls * 2, ll=ll * 2 if ll > 1 else 0))
    songlen = b[950]; order = list(b[952:952 + songlen]); assert b[1080:1084] == b"M.K."
    npat = max(b[952:1080]) + 1
    pdata = b[1084:1084 + npat * 1024]
    pos = 1084 + npat * 1024
    for s in sm:
        s['data'] = np.frombuffer(b[pos:pos + s['len']], np.int8).astype(np.float32) / 128; pos += s['len']
    chs = [dict(s=None, per=0, vol=0, pos=0.0, tgt=0, ps=0, vs=0, vd=0, vp=0, eff=0, par=0) for _ in range(4)]
    speed, bpm = 6, 125
    outL, outR = [], []
    pan = [0.75, 0.25, 0.25, 0.75]
    sine = np.sin(np.arange(64) / 64 * 2 * np.pi)
    for o in order:
        brk = False
        for row in range(64):
            for c in range(4):
                q = pdata[o * 1024 + row * 16 + c * 4: o * 1024 + row * 16 + c * 4 + 4]
                smp = (q[0] & 0xF0) | (q[2] >> 4); per = ((q[0] & 0x0F) << 8) | q[1]; eff = q[2] & 0xF; par = q[3]
                ch = chs[c]
                if smp: ch['s'] = smp - 1; ch['vol'] = sm[smp - 1]['vol']
                if per:
                    if eff == 3: ch['tgt'] = per
                    else: ch['per'] = per; ch['pos'] = 0.0; ch['vp'] = 0
                ch['eff'], ch['par'] = eff, par
                if eff == 3 and par: ch['ps'] = par
                if eff == 4:
                    if par >> 4: ch['vs'] = par >> 4
                    if par & 15: ch['vd'] = par & 15
                if eff == 0xC: ch['vol'] = min(par, 64)
                if eff == 0xF: speed, bpm = (par, bpm) if par < 32 else (speed, par)
                if eff == 0xD: brk = True
            for tick in range(speed):
                n = int(round(sr * 2.5 / bpm))
                L = np.zeros(n, np.float32); Rr = np.zeros(n, np.float32)
                for c, ch in enumerate(chs):
                    if tick and ch['eff'] == 3 and ch['tgt']:
                        if ch['per'] < ch['tgt']: ch['per'] = min(ch['per'] + ch['ps'], ch['tgt'])
                        else: ch['per'] = max(ch['per'] - ch['ps'], ch['tgt'])
                    per = ch['per']
                    if ch['eff'] == 4:
                        per = per + sine[ch['vp'] % 64] * ch['vd'] * 2
                        if tick: ch['vp'] += ch['vs']
                    if ch['s'] is None or not per or not ch['vol']:
                        if ch['s'] is not None and per:
                            ch['pos'] += PAL / (2 * per) / sr * n
                        continue
                    s = sm[ch['s']]; step = PAL / (2 * per) / sr
                    p = ch['pos'] + step * np.arange(n)
                    if s['ll']:
                        end = s['ls'] + s['ll']
                        p = np.where(p >= end, s['ls'] + np.mod(p - s['ls'], s['ll']), p)
                        idx = p.astype(int); v = s['data'][idx]
                    else:
                        idx = p.astype(int); ok = idx < s['len']
                        v = np.where(ok, s['data'][np.minimum(idx, s['len'] - 1)], 0)
                    ch['pos'] = float(ch['pos'] + step * n)
                    if s['ll'] and ch['pos'] >= s['ls'] + s['ll']:
                        ch['pos'] = s['ls'] + (ch['pos'] - s['ls']) % s['ll']
                    v = v * ch['vol'] / 64
                    L += v * pan[c]; Rr += v * (1 - pan[c])
                outL.append(L); outR.append(Rr)
            if brk: break
    st = np.stack([np.concatenate(outL), np.concatenate(outR)], 1) * 0.55
    st = np.tanh(st * 1.2)
    return st, sr

if __name__ == "__main__":
    out = write_mod(sys.argv[1])
    print("mod bytes:", len(out))
    st, sr = render(sys.argv[1])
    import wave
    w = wave.open(sys.argv[2], "wb"); w.setnchannels(2); w.setsampwidth(2); w.setframerate(sr)
    w.writeframes((st * 32000).astype(np.int16).tobytes()); w.close()
    print("seconds:", len(st) / sr, "peak:", float(np.abs(st).max()))
