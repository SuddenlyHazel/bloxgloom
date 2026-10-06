"""Regenerate linear unit-Lambert up/down BSL-base sky integrals.

Run: uv run --no-project --with numpy python tools/generate_sky_diffuse.py
No sun disc, cloud volume, voxel visibility, ground bounce, tone map or gamma.
Source equations: src/render/sky/bsl.wgsl::bg_bsl_sky_default.
The 3 stored coefficients reproduce its exact quadratic lunar dependence.
"""

import numpy as np
from pathlib import Path

N = 32768
u = (np.arange(N) + 0.5) / N
bits = np.arange(N, dtype=np.uint32)
bits = ((bits >> 1) & 0x55555555) | ((bits & 0x55555555) << 1)
bits = ((bits >> 2) & 0x33333333) | ((bits & 0x33333333) << 2)
bits = ((bits >> 4) & 0x0F0F0F0F) | ((bits & 0x0F0F0F0F) << 4)
bits = ((bits >> 8) & 0x00FF00FF) | ((bits & 0x00FF00FF) << 8)
bits = (bits >> 16) | (bits << 16)
phi = (bits.astype(np.float64) / 2**32) * 2 * np.pi
local = np.array([np.sqrt(u) * np.cos(phi), np.sqrt(u) * np.sin(phi), np.sqrt(1 - u)]).T
mix = lambda a, b, t: np.array(a) * (1 - np.array(t)) + np.array(b) * np.array(t)


def smooth(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


mid = np.array([-0.55, 0.65, -0.52])
mid /= np.linalg.norm(mid)


def bsl_ambient(sun, b, rain, moon):
    fade = 1 - (1 - np.clip(b, 0, 1)) ** 1.5
    day = mix(
        np.array([255, 204, 144]) * 0.35 / 255,
        np.array([120, 172, 255]) * 0.6 / 255,
        fade,
    )
    night = np.array([96, 192, 255]) * 0.18 / 255 * moon
    raw = mix(night, day, np.clip(sun[1] * 10 + 0.5, 0, 1))
    tint = mix(
        raw,
        np.dot(raw, [0.299, 0.587, 0.114]) * np.array([176, 224, 255]) * 1.2 / 255,
        rain,
    )
    return tint * tint


def sky(ray, sun, b, rain, moon):
    up = np.clip(ray[:, 1], -1, 1)
    toward = np.clip(ray @ sun, -1, 1)
    day = np.clip(sun[1] * 2 + 0.5, 0, 1)
    visible = np.clip(sun[1] * 10 + 0.5, 0, 1)
    curve = mix(1.5, 1, toward)
    grad = np.exp(-(1 - (1 - np.maximum(up, 0)) ** curve) / 0.35)
    val = (np.array([96, 160, 255]) / 255) ** 2 * grad[:, None]
    val = val / np.sqrt(val * val + 1) * 2 ** (b * 0.75 - 0.75) * day
    sunmix = ((toward * 0.5 + 0.5) * np.clip(1 - up, 0, 1)) ** (2 - day) * (
        1 - b * 0.6
    ) ** 3
    horizonmix = (1 - np.abs(up)) ** 2.5 * 0.125
    lm = 1 - (1 - sunmix) * (1 - horizonmix)
    fade = 1 - (1 - b) ** 1.5
    palette = (
        mix(np.array([255, 160, 80]) * 1.2, np.array([196, 220, 255]) * 1.4, fade) / 255
    )
    light = palette ** (4 - day) * grad[:, None]
    light /= 1 + light * rain
    val = (
        mix(
            np.sqrt(np.maximum(val * (1 - lm[:, None]), 0)), np.sqrt(light), lm[:, None]
        )
        ** 2
    )
    night = (
        (np.array([96, 192, 255]) * 0.3 / 255 * moon) ** 2
        * np.exp(-np.maximum(up, 0) / 0.65)[:, None]
        * 2**-3.5
    )
    val = mix(night, val, max(visible, day) ** 2)
    weather = (np.array([176, 224, 255]) * 1.2 / 255) ** 2
    weather *= np.dot(
        bsl_ambient(sun, b, rain, moon) / weather, [0.299, 0.587, 0.114]
    ) * (0.2 * day + 0.2)
    val = mix(val, weather * np.exp(-np.maximum(up, 0) / 1.5)[:, None], rain)
    gu = np.clip(-up * 1.015 - 0.015, 0, 1)
    gd = 0.1 * (4 - 3 * day) * (10 * rain * rain + 1)
    val *= (-np.expm1(-gd / np.maximum(gu, 1e-6)))[:, None]
    # Actual bg_sky_environment has no artificial ground. Solar disc is excluded.
    val *= smooth(-0.08, 0, ray[:, 1])[:, None]
    return np.maximum(val, 0)


def main():
    maximum = mid[1]
    elevations = np.unique(
        np.r_[
            np.linspace(-maximum, maximum, 65),
            [-0.25, -0.12, -0.05, 0.05, 0.12, 0.18, 0.25],
        ]
    )
    rows = []
    for elevation in elevations:
        sun = np.array([-np.sqrt(1 - elevation**2), elevation, 0])
        brightness = np.clip(elevation / maximum, 0, 1)
        for rain in np.linspace(0, 1, 9):
            values = []
            for moon in [0, 0.5, 1]:
                values.append(
                    np.concatenate(
                        [
                            sky(
                                local @ np.array([[1, 0, 0], [0, 0, -1], [0, 1, 0]]),
                                sun,
                                brightness,
                                rain,
                                moon,
                            ).mean(axis=0),
                            sky(
                                local @ np.array([[1, 0, 0], [0, 0, 1], [0, -1, 0]]),
                                sun,
                                brightness,
                                rain,
                                moon,
                            ).mean(axis=0),
                        ]
                    )
                )
            # f(m)=a+b*m+c*m*m, evaluated exactly from 0, 1/2 and 1.
            a = values[0]
            c = 2 * (values[2] - 2 * values[1] + values[0])
            b = values[2] - a - c
            rows.extend([a, b, c])
    out = Path(__file__).resolve().parent.parent / "src/render/daylight"
    (out / "sky_diffuse.bin").write_bytes(
        np.asarray([len(elevations)], dtype="<u4").tobytes()
        + np.asarray(elevations, dtype="<f4").tobytes()
        + np.asarray(rows, dtype="<f4").tobytes()
    )
    print("elevations:", ",".join(f"{x:.9f}" for x in elevations))
    print("bytes:", (out / "sky_diffuse.bin").stat().st_size)


if __name__ == "__main__":
    main()
