"""Generate compact SH9 unit-Lambert BSL-base sky coefficients.

Run: uv run --no-project --with numpy python tools/generate_sky_angular.py
The sky is reflection-symmetric about its sun/up plane, leaving six nonzero
real SH9 terms. Stored quadratic polynomials evaluate in that canonical frame;
no sun disc, clouds, invented ground, local light or presentation is included.
"""
from pathlib import Path
import numpy as np
import generate_sky_diffuse as source

N = 32768
u = (np.arange(N) + 0.5) / N
bits = np.arange(N, dtype=np.uint32)
for shift, mask in [(1, 0x55555555), (2, 0x33333333), (4, 0x0F0F0F0F), (8, 0x00FF00FF)]:
    bits = ((bits >> shift) & mask) | ((bits & mask) << shift)
bits = (bits >> 16) | (bits << 16)
phi = bits.astype(np.float64) / 2**32 * (2 * np.pi)
y = 1 - 2 * u
x = np.sqrt(1 - y * y) * np.cos(phi)
z = np.sqrt(1 - y * y) * np.sin(phi)
rays = np.array([x, y, z]).T
# Orthonormal real SH basis, with its reflection-odd Z terms omitted exactly.
basis = np.array([
    np.ones(N) * np.sqrt(1 / (4 * np.pi)),
    x * np.sqrt(3 / (4 * np.pi)),
    y * np.sqrt(3 / (4 * np.pi)),
    x * y * np.sqrt(15 / (4 * np.pi)),
    (3 * y * y - 1) * np.sqrt(5 / (16 * np.pi)),
    (x * x - z * z) * np.sqrt(15 / (16 * np.pi)),
])
# Cosine convolution divided by pi: outgoing radiance for unit Lambert albedo.
basis *= np.array([1, 2 / 3, 2 / 3, 1 / 4, 1 / 4, 1 / 4])[:, None]


def polynomial(radiance):
    coeff = (basis @ radiance) * (4 * np.pi / N)
    c0, cx, cy, cxy, cyy, cxx = coeff
    syy = np.sqrt(5 / (16 * np.pi))
    sxx = np.sqrt(15 / (16 * np.pi))
    return np.array([
        c0 * np.sqrt(1 / (4 * np.pi)) - cyy * syy - cxx * sxx,
        cx * np.sqrt(3 / (4 * np.pi)),
        cy * np.sqrt(3 / (4 * np.pi)),
        cxy * np.sqrt(15 / (4 * np.pi)),
        cyy * 3 * syy + cxx * sxx,
        cxx * 2 * sxx,
    ])


def main():
    maximum = source.mid[1]
    elevations = np.unique(np.r_[np.linspace(-maximum, maximum, 65),
        [-1, -.9, -.8, -.7, -.25, -.12, -.05, .05, .12, .18, .25, .7, .8, .9, 1]])
    rows = []
    for elevation in elevations:
        sun = np.array([-np.sqrt(max(1 - elevation * elevation, 0)), elevation, 0])
        brightness = np.clip(elevation / maximum, 0, 1)
        for rain in np.linspace(0, 1, 9):
            values = [polynomial(source.sky(rays, sun, brightness, rain, moon)) for moon in [0, .5, 1]]
            a = values[0]
            c = 2 * (values[2] - 2 * values[1] + a)
            b = values[2] - a - c
            rows.extend([a, b, c])
    out = Path(__file__).resolve().parent.parent / 'src/render/daylight/sky_diffuse'
    out.mkdir(exist_ok=True)
    data = np.asarray([len(elevations)], dtype='<u4').tobytes()
    data += np.asarray(elevations, dtype='<f4').tobytes()
    data += np.asarray(rows, dtype='<f4').tobytes()
    (out / 'angular.bin').write_bytes(data)
    print(f'{len(elevations)} elevations, {len(data)} bytes; six RGB polynomial terms')


if __name__ == '__main__':
    main()
