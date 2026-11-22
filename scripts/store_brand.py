# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Store builds take the app name, icons and app id from the licensee's server, so nobody ships the built-in brand.

  python -I scripts/store_brand.py customer|partner >> "$GITHUB_ENV"

Reads GET {API_BASE}/v1/brand (A: profile + `assets` with absolute public URLs) and writes into app/apps/<app>:
  - Android: legacy launcher icons from `store_1024`, the adaptive foreground and monochrome layers from `icon_512`,
    and the adaptive background colour from `primary_color`;
  - iOS: every AppIcon size from `store_1024`, flattened to RGB (App Store Connect rejects an alpha channel);
  - Windows: the window title (runner/main.cpp) and ProductName / FileDescription (APP_LABEL) and
    CompanyName (the brand name) in runner/Runner.rc.
Prints APP_LABEL (customer: short_name; partner: "<short_name> 파트너") and APP_ID for the build steps
(Gradle reads them, native-keys.sh writes them into Keys.xcconfig).

Env: API_BASE (https), CUSTOMER_APP_ID / PARTNER_APP_ID (default kr.co.dahae.<app>),
     BRAND_FROM_SERVER (default true; false keeps the repo's name and icons, for a build before the server is up).
A server that cannot be reached, or a profile without the store icon, fails the build.
"""
import io
import json
import os
import pathlib
import re
import sys
import urllib.request

from PIL import Image

REPO = pathlib.Path(__file__).resolve().parents[1]
APPS = REPO / "app" / "apps"
DENSITIES = {"mdpi": 1, "hdpi": 1.5, "xhdpi": 2, "xxhdpi": 3, "xxxhdpi": 4}
ADAPTIVE_SAFE = 0.44  # 108 dp canvas: only the centred 66 dp circle is never masked (scripts/app_icons.py)
SHORT_MAX = 12
MAX_ASSET_BYTES = 5 * 1024 * 1024
TIMEOUT_S = 20
# One id serves as the Android applicationId and the iOS bundle id: Android refuses `-`, iOS refuses `_`.
APP_ID_RE = re.compile(r"[A-Za-z][A-Za-z0-9]*(\.[A-Za-z][A-Za-z0-9]*)+")
HEX_RE = re.compile(r"#[0-9A-Fa-f]{6}")
LANCZOS = Image.Resampling.LANCZOS


def app_id(app, env):
    value = (env.get(f"{app.upper()}_APP_ID") or "").strip() or f"kr.co.dahae.{app}"
    if not APP_ID_RE.fullmatch(value) or len(value) > 150:
        sys.exit(f"{app.upper()}_APP_ID is not a valid app id: {value!r}")
    return value


def clean(text):
    """Drops characters that would break the manifest, xcconfig, or a C++ or .rc string literal."""
    # A leading @ or ? would make android:label a resource reference.
    return re.sub(r"[\x00-\x1f<>&\"'\\$/]", "", text).strip().lstrip("@? ")


def label(app, brand):
    """Home-screen name."""
    name = clean(str(brand.get("short_name") or "").strip() or str(brand.get("name") or "").strip()[:SHORT_MAX])
    if not name:
        sys.exit("/v1/brand has no name")
    return name if app == "customer" else f"{name} 파트너"


def fetch(url):
    if not url.startswith("https://"):
        sys.exit(f"refusing a non-https address: {url}")
    with urllib.request.urlopen(url, timeout=TIMEOUT_S) as resp:  # noqa: S310 (https only, checked above)
        if not resp.url.startswith("https://"):
            sys.exit(f"refusing a redirect off https: {url}")
        body = resp.read(MAX_ASSET_BYTES + 1)
    if len(body) > MAX_ASSET_BYTES:
        sys.exit(f"{url}: larger than {MAX_ASSET_BYTES} bytes")
    return body


def image(brand, slot, size, get):
    url = (brand.get("assets") or {}).get(slot)
    if not url:
        sys.exit(f"/v1/brand has no {slot} icon: save the logo in the console (사업자 정보) first")
    # PNG only: this job later decodes the signing keys, so no other Pillow decoder sees fetched bytes.
    try:
        im = Image.open(io.BytesIO(get(url)), formats=["PNG"])
    except (OSError, SyntaxError) as e:
        sys.exit(f"{slot}: not a PNG image ({e})")
    if im.size != (size, size):
        sys.exit(f"{slot}: expected {size}x{size}, got {im.size[0]}x{im.size[1]}")
    return im.convert("RGBA")


def placed(mark, px, box):
    """Mark scaled into a centred box (fraction of a transparent square canvas)."""
    k = box * px / max(mark.size)
    m = mark.resize((max(1, round(mark.width * k)), max(1, round(mark.height * k))), LANCZOS)
    canvas = Image.new("RGBA", (px, px), (0, 0, 0, 0))
    canvas.alpha_composite(m, ((px - m.width) // 2, (px - m.height) // 2))
    return canvas


def save(im, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    im.save(path, optimize=True)


def android_icons(res, store, icon, color):
    mono = Image.new("RGBA", icon.size, (255, 255, 255, 0))
    mono.putalpha(icon.getchannel("A"))  # the logo's shape in white: Android tints it for themed icons
    for d, k in DENSITIES.items():
        save(store.resize((round(48 * k),) * 2, LANCZOS), res / f"mipmap-{d}" / "ic_launcher.png")
        save(placed(icon, round(108 * k), ADAPTIVE_SAFE), res / f"mipmap-{d}" / "ic_launcher_foreground.png")
        save(placed(mono, round(108 * k), ADAPTIVE_SAFE), res / f"mipmap-{d}" / "ic_launcher_monochrome.png")
    colors = res / "values" / "colors.xml"
    text = colors.read_text(encoding="utf-8")
    new = re.sub(r'(<color name="ic_launcher_background">)[^<]*(</color>)', rf"\g<1>{color}\g<2>", text)
    if new == text and color not in text:
        sys.exit(f"{colors}: no ic_launcher_background colour to set")
    colors.write_text(new, encoding="utf-8", newline="\n")


def ios_icons(iconset, store):
    flat = Image.new("RGB", store.size, (255, 255, 255))
    flat.paste(store, mask=store.getchannel("A"))
    for png in iconset.glob("*.png"):
        with Image.open(png) as old:
            px = old.size[0]
        save(flat.resize((px, px), LANCZOS), png)


def wide(text):
    """C++ wide literal body: non-ASCII as \\u escapes, so MSVC reads it the same under any code page."""
    return "".join(c if ord(c) < 0x80 else f"\\u{ord(c):04X}" if ord(c) <= 0xFFFF else f"\\U{ord(c):08X}" for c in text)


def sub_once(path, pattern, value):
    text = path.read_text(encoding="utf-8")
    new, n = re.subn(pattern, lambda m: m.group(1) + value + m.group(2), text)
    if n != 1:
        sys.exit(f"{path}: expected one {pattern!r}, found {n}")
    path.write_text(new, encoding="utf-8", newline="\n")


def windows_names(runner, title, company):
    """Window title (main.cpp) and the exe's version info (Runner.rc, #pragma code_page(65001): plain UTF-8)."""
    sub_once(runner / "main.cpp", r'(window\.Create\(L")[^"]*(")', wide(title))
    for key, value in (("CompanyName", company), ("FileDescription", title), ("ProductName", title)):
        sub_once(runner / "Runner.rc", rf'(VALUE "{key}", ")[^"]*(")', value)


def apply(app, brand, get, apps=APPS):
    """Writes the icons into the app folder and returns the APP_LABEL value."""
    color = str(brand.get("primary_color") or "")
    if not HEX_RE.fullmatch(color):
        sys.exit(f"/v1/brand primary_color is not #RRGGBB: {color!r}")
    store, icon = image(brand, "store_1024", 1024, get), image(brand, "icon_512", 512, get)
    android_icons(apps / app / "android" / "app" / "src" / "main" / "res", store, icon, color.upper())
    ios_icons(apps / app / "ios" / "Runner" / "Assets.xcassets" / "AppIcon.appiconset", store)
    name = label(app, brand)
    windows_names(apps / app / "windows" / "runner", name, clean(str(brand.get("name") or "")) or name)
    return name


def main(argv, env):
    if len(argv) != 2 or argv[1] not in ("customer", "partner"):
        sys.exit("usage: store_brand.py customer|partner")
    app = argv[1]
    lines = [f"APP_ID={app_id(app, env)}"]
    if (env.get("BRAND_FROM_SERVER") or "true").strip().lower() != "false":
        base = (env.get("API_BASE") or "").rstrip("/")
        try:
            brand = json.loads(fetch(f"{base}/v1/brand"))
        except (OSError, ValueError) as e:
            sys.exit(f"could not read {base}/v1/brand ({e}). Fix API_BASE or run with brand_from_server off.")
        lines.append(f"APP_LABEL={apply(app, brand, fetch)}")
    print("\n".join(lines))


if __name__ == "__main__":
    main(sys.argv, os.environ)
