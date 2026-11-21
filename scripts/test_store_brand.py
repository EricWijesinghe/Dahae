# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""scripts/store_brand.py on a fixture brand, no network: labels, app ids, icon sizes, RGB iOS icons.

  python -I scripts/test_store_brand.py      (needs Pillow, like store_brand.py)
"""
import contextlib
import io
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import store_brand  # noqa: E402

from PIL import Image  # noqa: E402

COLORS = '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n    <color name="ic_launcher_background">#1E9EBA</color>\n' \
         '    <color name="splash_background">#F7FAFB</color>\n</resources>\n'


def png(size, rgba):
    buf = io.BytesIO()
    Image.new("RGBA", (size, size), rgba).save(buf, format="PNG")
    return buf.getvalue()


class StoreBrand(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.apps = pathlib.Path(self.tmp.name)
        for app in ("customer", "partner"):
            values = self.apps / app / "android" / "app" / "src" / "main" / "res" / "values"
            values.mkdir(parents=True)
            (values / "colors.xml").write_text(COLORS, encoding="utf-8")
            iconset = self.apps / app / "ios" / "Runner" / "Assets.xcassets" / "AppIcon.appiconset"
            iconset.mkdir(parents=True)
            for px in (40, 1024):
                Image.new("RGBA", (px, px), (0, 0, 0, 0)).save(iconset / f"Icon-{px}.png")
        self.files = {"https://x/store.png": png(1024, (24, 122, 143, 255)), "https://x/icon.png": png(512, (255, 0, 0, 128))}
        self.brand = {"name": "청소왕 서비스", "short_name": "청소왕", "primary_color": "#187a8f",
                      "assets": {"store_1024": "https://x/store.png", "icon_512": "https://x/icon.png"}}

    def tearDown(self):
        self.tmp.cleanup()

    def run_apply(self, app, brand=None):
        return store_brand.apply(app, brand or self.brand, self.files.__getitem__, apps=self.apps)

    def test_labels_follow_the_short_name(self):
        self.assertEqual(self.run_apply("customer"), "청소왕")
        self.assertEqual(self.run_apply("partner"), "청소왕 파트너")
        no_short = dict(self.brand, short_name="")
        self.assertEqual(self.run_apply("customer", no_short), "청소왕 서비스")
        odd = dict(self.brand, short_name='A&B<"x">')
        self.assertEqual(self.run_apply("customer", odd), "ABx")

    def test_icons_have_every_size_and_ios_is_opaque(self):
        self.run_apply("customer")
        res = self.apps / "customer" / "android" / "app" / "src" / "main" / "res"
        self.assertEqual(Image.open(res / "mipmap-xxxhdpi" / "ic_launcher.png").size, (192, 192))
        self.assertEqual(Image.open(res / "mipmap-mdpi" / "ic_launcher_foreground.png").size, (108, 108))
        mono = Image.open(res / "mipmap-hdpi" / "ic_launcher_monochrome.png").convert("RGBA")
        self.assertEqual(mono.getpixel((81, 81))[:3], (255, 255, 255), "monochrome is a white silhouette")
        colors = (res / "values" / "colors.xml").read_text(encoding="utf-8")
        self.assertIn('<color name="ic_launcher_background">#187A8F</color>', colors)
        self.assertIn("splash_background", colors, "other colours stay")
        iconset = self.apps / "customer" / "ios" / "Runner" / "Assets.xcassets" / "AppIcon.appiconset"
        big = Image.open(iconset / "Icon-1024.png")
        self.assertEqual((big.mode, big.size), ("RGB", (1024, 1024)))
        self.assertEqual(Image.open(iconset / "Icon-40.png").mode, "RGB")

    def test_missing_or_wrong_icons_fail_the_build(self):
        with self.assertRaises(SystemExit):
            self.run_apply("customer", dict(self.brand, assets={"icon_512": "https://x/icon.png"}))
        self.files["https://x/store.png"] = png(512, (0, 0, 0, 255))
        with self.assertRaises(SystemExit):
            self.run_apply("customer")
        with self.assertRaises(SystemExit):
            self.run_apply("customer", dict(self.brand, primary_color="teal"))

    def test_app_ids_come_from_the_variables(self):
        self.assertEqual(store_brand.app_id("customer", {}), "kr.co.dahae.customer")
        self.assertEqual(store_brand.app_id("partner", {"PARTNER_APP_ID": " com.shop.partner "}), "com.shop.partner")
        for bad in ("com.my-shop.app", "noDots", "com.1shop"):
            with self.assertRaises(SystemExit):
                store_brand.app_id("customer", {"CUSTOMER_APP_ID": bad})

    def test_off_switch_prints_only_the_app_id(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            store_brand.main(["store_brand.py", "customer"], {"BRAND_FROM_SERVER": "false", "CUSTOMER_APP_ID": "com.shop.app"})
        self.assertEqual(out.getvalue(), "APP_ID=com.shop.app\n")

    def test_a_plain_http_server_is_refused(self):
        with self.assertRaises(SystemExit):
            store_brand.main(["store_brand.py", "customer"], {"API_BASE": "http://shop.example"})


if __name__ == "__main__":
    unittest.main()
