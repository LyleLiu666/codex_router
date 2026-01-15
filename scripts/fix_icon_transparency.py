#!/usr/bin/env python3
"""
scripts/fix_icon_transparency.py

This script provides two methods to fix icon transparency issues:

================================================================================
METHOD 1: GRAY REMOVAL (for images with baked-in gray checkerboard background)
================================================================================
Use this when you have an existing image where the background is a gray
checkerboard pattern (e.g., from an image editor export issue).

Algorithm:
- Identifies "grayish" pixels: low saturation (max-min < 30)
- Protects bright white highlights (brightness > 245)
- Makes matching gray pixels fully transparent

Command:
    python3 scripts/fix_icon_transparency.py assets/icon.png

================================================================================
METHOD 2: BLACK TO ALPHA (RECOMMENDED for new image generation)
================================================================================
Use this when generating new icons. Generate the icon on a PURE BLACK background,
then use this method to convert the black to transparency.

This is the PREFERRED method because:
- Black background is easy for AI image generators to produce
- Luminance-to-Alpha conversion perfectly preserves glow/neon effects
- No edge artifacts or "halos"

Algorithm:
- Alpha = max(R, G, B)  (luminance determines transparency)
- Color is "un-premultiplied" to restore original brightness
- Pure black (0,0,0) becomes fully transparent

Command:
    python3 scripts/fix_icon_transparency.py assets/icon.png --black-bg

================================================================================
USAGE EXAMPLES
================================================================================
# Fix existing image with gray checkerboard:
python3 scripts/fix_icon_transparency.py assets/icon.png

# Convert new black-background image to transparent:
python3 scripts/fix_icon_transparency.py assets/icon.png --black-bg

================================================================================
DEPENDENCIES
================================================================================
Pillow (pip install Pillow)
"""

from PIL import Image
import os
import sys


def fix_gray_background(input_path, output_path):
    """
    Method 1: Remove gray checkerboard background.
    Good for existing images with baked-in gray backgrounds.
    """
    print(f"[Gray Removal] Processing {input_path}...")
    
    img = Image.open(input_path).convert('RGBA')
    pixels = img.load()
    width, height = img.size

    SATURATION_THRESHOLD = 30
    BRIGHTNESS_THRESHOLD_WHITE = 245
    BRIGHTNESS_THRESHOLD_BLACK = 20

    count = 0
    for y in range(height):
        for x in range(width):
            r, g, b, a = pixels[x, y]
            
            if a == 0:
                continue

            saturation = max(r, g, b) - min(r, g, b)
            brightness = max(r, g, b)

            is_gray = saturation < SATURATION_THRESHOLD
            is_too_bright = brightness > BRIGHTNESS_THRESHOLD_WHITE
            is_too_dark = brightness < BRIGHTNESS_THRESHOLD_BLACK 
            
            if is_gray and not is_too_bright and not is_too_dark:
                pixels[x, y] = (0, 0, 0, 0)
                count += 1

    print(f"Made {count} pixels transparent.")
    img.save(output_path)
    print(f"Saved to {output_path}")


def fix_black_background(input_path, output_path):
    """
    Method 2: Convert black background to transparency using luminance.
    RECOMMENDED for newly generated images on black backgrounds.
    
    This perfectly preserves glowing/neon effects.
    """
    print(f"[Black-to-Alpha] Processing {input_path}...")
    
    img = Image.open(input_path).convert('RGB')
    width, height = img.size
    new_img = Image.new('RGBA', (width, height))
    pixels_in = img.load()
    pixels_out = new_img.load()

    for y in range(height):
        for x in range(width):
            r, g, b = pixels_in[x, y]
            
            # Alpha = luminance (max of RGB channels)
            alpha = max(r, g, b)
            
            if alpha > 0:
                # Un-premultiply to restore original color brightness
                rn = min(255, int(r * 255 / alpha))
                gn = min(255, int(g * 255 / alpha))
                bn = min(255, int(b * 255 / alpha))
                pixels_out[x, y] = (rn, gn, bn, alpha)
            else:
                pixels_out[x, y] = (0, 0, 0, 0)

    new_img.save(output_path)
    print(f"Saved to {output_path}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 fix_icon_transparency.py <image_path> [--black-bg]")
        print("  --black-bg : Use black-to-alpha method (recommended for new generations)")
        sys.exit(1)
    
    input_path = sys.argv[1]
    use_black_bg_method = "--black-bg" in sys.argv
    
    if not os.path.exists(input_path):
        print(f"File not found: {input_path}")
        sys.exit(1)

    if use_black_bg_method:
        fix_black_background(input_path, input_path)
    else:
        fix_gray_background(input_path, input_path)
