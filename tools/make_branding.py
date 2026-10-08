"""Build the original application icon (requires Pillow)."""

from pathlib import Path

from PIL import Image, ImageDraw


def main() -> None:
    root = Path(__file__).resolve().parents[1] / "assets"
    if not root.is_dir():
        raise SystemExit("assets/ is missing")
    scale = 4
    image = Image.new("RGBA", (256 * scale, 256 * scale))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((8 * scale, 8 * scale, 248 * scale, 248 * scale), 54 * scale, fill="#86B9E8")
    for points in [
        [(128, 54), (128, 148)],
        [(86, 108), (128, 150), (170, 108)],
        [(62, 157), (62, 197), (194, 197), (194, 157)],
    ]:
        draw.line([(x * scale, y * scale) for x, y in points], fill="#121518", width=13 * scale, joint="curve")
    icon = image.resize((256, 256), Image.Resampling.LANCZOS)
    icon.save(root / "SoftDownloader.png", optimize=True)
    icon.save(root / "SoftDownloader.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    print("Built assets/SoftDownloader.png and assets/SoftDownloader.ico")


if __name__ == "__main__":
    main()
