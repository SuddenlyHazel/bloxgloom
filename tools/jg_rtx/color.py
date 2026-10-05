"""Linear-light, alpha-aware operations on sRGB albedo images."""
import numpy as np
from PIL import Image


def srgb_to_linear(value):
    return np.where(value <= .04045, value / 12.92, ((value + .055) / 1.055) ** 2.4)


def linear_to_srgb(value):
    value = np.clip(value, 0., 1.)
    return np.where(value <= .0031308, value * 12.92, 1.055 * value ** (1 / 2.4) - .055)


def rgba_from_linear(rgb, alpha):
    return Image.fromarray(np.concatenate((
        np.rint(linear_to_srgb(rgb) * 255).astype(np.uint8),
        np.rint(np.clip(alpha, 0., 1.) * 255).astype(np.uint8)[:, :, None],
    ), axis=2))


def resize(image, size):
    image = image.convert('RGBA')
    if image.size == size:
        return image.copy()
    data = np.array(image).astype(float) / 255
    alpha = data[:, :, 3]
    rgb = srgb_to_linear(data[:, :, :3]) * alpha[:, :, None]

    def resample(channel):
        return np.array(Image.fromarray(channel.astype(np.float32)).resize(
            size, Image.Resampling.LANCZOS)).astype(float)

    alpha = np.clip(resample(alpha), 0., 1.)
    rgb = np.stack([resample(rgb[:, :, channel]) for channel in range(3)], axis=2)
    rgb = np.divide(rgb, alpha[:, :, None], out=np.zeros_like(rgb), where=alpha[:, :, None] > 1e-8)
    return rgba_from_linear(rgb, alpha)


def composite(base, overlay, position=(0, 0)):
    """Straight-alpha source-over, with color arithmetic in linear light."""
    foreground = Image.new('RGBA', base.size)
    foreground.paste(overlay, position)
    background = np.array(base.convert('RGBA')).astype(float) / 255
    foreground = np.array(foreground).astype(float) / 255
    a = foreground[:, :, 3]
    b = background[:, :, 3]
    alpha = a + b * (1 - a)
    rgb = (srgb_to_linear(foreground[:, :, :3]) * a[:, :, None]
           + srgb_to_linear(background[:, :, :3]) * (b * (1 - a))[:, :, None])
    rgb = np.divide(rgb, alpha[:, :, None], out=np.zeros_like(rgb), where=alpha[:, :, None] > 1e-8)
    return rgba_from_linear(rgb, alpha)
