"""The use-case videos' sound, made here from sine waves and noise (our own,
under the project's licence): a quiet music bed and five UI sounds.

Usage: tools/.venv/bin/python tools/gen_sound.py OUT_DIR

Writes music.wav (stereo, 48 kHz, 48 s: Cmaj7, Am7, Fmaj7, G6 at 84 bpm, a
soft pad, a slow pluck arpeggio and a sub bass) and sting.wav, pickup.wav,
drop.wav, click.wav, reveal.wav and end.wav (mono). The same files every
time (fixed seed). tools/demo.mjs mixes them with ffmpeg at the moments the
recording really did something.
"""

import sys
import wave
from pathlib import Path

import numpy as np

RATE = 48000
RNG = np.random.default_rng(20261021)


def note(name):
    """Frequency of a note such as "A4" or "C#5"."""
    names = {"C": -9, "C#": -8, "D": -7, "D#": -6, "E": -5, "F": -4, "F#": -3, "G": -2, "G#": -1, "A": 0,
             "A#": 1, "B": 2}
    return 440.0 * 2 ** ((names[name[:-1]] + 12 * (int(name[-1]) - 4)) / 12)


def t_axis(seconds):
    return np.arange(int(seconds * RATE)) / RATE


def lowpass(x, cutoff):
    """A gentle one-pole low-pass."""
    a = np.exp(-2 * np.pi * cutoff / RATE)
    y = np.empty_like(x)
    acc = 0.0
    for i, v in enumerate(x):
        acc = (1 - a) * v + a * acc
        y[i] = acc
    return y


def envelope(n, attack, release, total=None):
    total = total or n / RATE
    t = np.arange(n) / RATE
    env = np.minimum(1.0, t / max(attack, 1e-4))
    env *= np.clip((total - t) / max(release, 1e-4), 0, 1)
    return env


def pad(freqs, seconds, detune=0.003):
    t = t_axis(seconds)
    left = sum(np.sin(2 * np.pi * f * (1 - detune) * t) + 0.3 * np.sin(4 * np.pi * f * t) for f in freqs)
    right = sum(np.sin(2 * np.pi * f * (1 + detune) * t) + 0.3 * np.sin(4 * np.pi * f * t) for f in freqs)
    env = envelope(len(t), 0.9, 1.2)
    return np.stack([left * env, right * env], axis=1) / len(freqs)


def pluck(f, seconds=1.4):
    t = t_axis(seconds)
    tone = np.sin(2 * np.pi * f * t) + 0.25 * np.sin(4 * np.pi * f * t) + 0.08 * np.sin(6 * np.pi * f * t)
    return tone * np.exp(-t * 3.2) * np.minimum(1, t / 0.004)


def music(seconds=48.0):
    beat = 60 / 84
    bar = 4 * beat
    chords = [
        ["C3", "E3", "G3", "B3"], ["A2", "C3", "E3", "G3"], ["F2", "A2", "C3", "E3"], ["G2", "B2", "D3", "E3"],
    ]
    out = np.zeros((int(seconds * RATE) + RATE * 10, 2))
    t0 = 0.0
    k = 0
    while t0 < seconds:
        chord = chords[k % len(chords)]
        length = 2 * bar
        freqs = [note(n) for n in chord]
        p = pad(freqs, length + 1.0)
        i = int(t0 * RATE)
        out[i:i + len(p)] += 0.55 * p
        # Sub bass on the root.
        tb = t_axis(length + 0.5)
        bass = np.sin(2 * np.pi * freqs[0] / 2 * tb) * envelope(len(tb), 0.3, 0.8)
        out[i:i + len(bass)] += 0.35 * bass[:, None]
        # A slow arpeggio an octave up, on eighth notes, every other one.
        for e in range(0, int(length / (beat / 2)), 2):
            f = freqs[(e // 2) % len(freqs)] * 2
            pl = pluck(f)
            j = int((t0 + e * beat / 2) * RATE)
            pan = 0.35 if (e // 2) % 2 else -0.35
            out[j:j + len(pl), 0] += 0.16 * pl * (1 - pan)
            out[j:j + len(pl), 1] += 0.16 * pl * (1 + pan)
        t0 += length
        k += 1
    out = out[: int(seconds * RATE)]
    out[:, 0] = lowpass(out[:, 0], 2400)
    out[:, 1] = lowpass(out[:, 1], 2400)
    return out / np.max(np.abs(out)) * 0.7


def chime(freqs, seconds, gap=0.08, decay=3.5):
    out = np.zeros(int(seconds * RATE))
    for k, f in enumerate(freqs):
        t = t_axis(seconds - k * gap)
        tone = (np.sin(2 * np.pi * f * t) + 0.3 * np.sin(2 * np.pi * 2.01 * f * t)) * np.exp(-t * decay)
        tone *= np.minimum(1, t / 0.003)
        i = int(k * gap * RATE)
        out[i:i + len(tone)] += tone
    return out / np.max(np.abs(out)) * 0.8


def sounds():
    t = t_axis(0.3)
    # Drop: a soft, low thump with a falling pitch.
    f = 120 * np.exp(-t * 4)
    thump = np.sin(2 * np.pi * np.cumsum(f) / RATE) * np.exp(-t * 14)
    thump += 0.15 * lowpass(RNG.standard_normal(len(t)), 900) * np.exp(-t * 40)
    # Click: a short, bright tick.
    tc = t_axis(0.06)
    click = (np.sin(2 * np.pi * 2200 * tc) * 0.6 + 0.4 * RNG.standard_normal(len(tc))) * np.exp(-tc * 120)
    # Pickup: a soft rising swish.
    tp = t_axis(0.22)
    swish = lowpass(RNG.standard_normal(len(tp)), 2500) * np.sin(np.pi * tp / 0.22) ** 2
    return {
        "drop": thump / np.max(np.abs(thump)) * 0.85,
        "click": click / np.max(np.abs(click)) * 0.5,
        "pickup": swish / np.max(np.abs(swish)) * 0.35,
        "reveal": chime([note("G5"), note("D6")], 1.0),
        "sting": chime([note("E5"), note("B5"), note("E6")], 1.6, gap=0.12, decay=2.4),
        "end": chime([note("C5"), note("E5"), note("G5"), note("C6")], 2.2, gap=0.06, decay=1.6),
    }


def write(path, data):
    data = np.clip(data, -1, 1)
    channels = 1 if data.ndim == 1 else data.shape[1]
    with wave.open(str(path), "wb") as w:
        w.setnchannels(channels)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes((data * 32767).astype("<i2").tobytes())


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    write(out / "music.wav", music())
    for name, data in sounds().items():
        write(out / f"{name}.wav", data)


if __name__ == "__main__":
    main()
