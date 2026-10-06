"""Conservative inferred relief following JG RTX's labpbr/height.ts.

This periodic regularized Poisson solve estimates height from canonical RGB
normals. It cannot recover nonintegrable artwork or absolute geometry; fitting
confidence and physical depth bounds keep those cases out of imported assets.
"""
import re

import numpy as np


def structural(name):
    return bool(re.search(
        r'(^log_|_log(?:_side|_top)?$|^stripped_|^planks_|_planks$|brick|'
        r'^stone$|^cobblestone|^deepslate|^blackstone|^granite|^diorite|^andesite|^tuff)',
        name))


def reconstruct(normal):
    """Return quarter-block LabPBR alpha only when normal integration is sound."""
    normal = np.asarray(normal, dtype=float)[:, :, :3] / 127.5 - 1
    rows, columns = normal.shape[:2]
    gradients = -normal[:, :, :2] / np.maximum(normal[:, :, 2:], .05)
    gradients *= np.minimum(1, 4 / np.maximum(np.linalg.norm(gradients, axis=2, keepdims=True), 1e-8))
    energy = float(np.sum(gradients * gradients))
    if energy < 1e-8:
        return None, {'normal_fit': 0., 'depth_fraction': 0.}
    wx = 2 * np.pi * np.fft.fftfreq(columns)[None, :]
    wy = 2 * np.pi * np.fft.fftfreq(rows)[:, None]
    spectrum = (-1j * wx * np.fft.fft2(gradients[:, :, 0])
                - 1j * wy * np.fft.fft2(gradients[:, :, 1])) / (wx * wx + wy * wy + .012)
    field = np.fft.ifft2(spectrum).real
    fitted = np.stack([np.fft.ifft2(1j * wx * spectrum).real,
                       np.fft.ifft2(1j * wy * spectrum).real], axis=2)
    confidence = 1 - float(np.sum((gradients - fitted) ** 2)) / energy
    depth = float(np.ptp(field)) / columns
    details = {'normal_fit': round(confidence, 6), 'depth_fraction': round(depth, 6),
               'method': 'inferred periodic regularized Poisson; slope cap 4; lambda 0.012'}
    if confidence < .70 or depth < .001 or depth > .125:
        return None, details
    alpha = np.clip(np.rint(255 * (1 - (field.max() - field) / (.25 * columns))), 1, 255).astype(np.uint8)
    return alpha, details
