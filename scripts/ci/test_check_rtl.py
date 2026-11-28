# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Tests for check-rtl.py. Run: python3 -I -m unittest discover -s scripts/ci -p 'test_*.py'"""

import importlib.util
import pathlib
import unittest

_spec = importlib.util.spec_from_file_location("check_rtl", pathlib.Path(__file__).with_name("check-rtl.py"))
cr = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cr)


class Hits(unittest.TestCase):
    def test_left_right_only_apis_are_flagged(self):
        src = "\n".join([
            "a(padding: const EdgeInsets.fromLTRB(1, 2, 3, 4));",
            "b(padding: const EdgeInsets.only(\n  left: 4,\n  top: 2,\n));",
            "Positioned(right: 0, child: x)",
            "c(alignment: Alignment.centerLeft);",
            "d(textAlign: TextAlign.right);",
            "BorderRadius.only(topLeft: r)",
        ])
        self.assertEqual(
            [line for line, _ in cr.hits(src)], [1, 2, 6, 7, 8, 9],
        )

    def test_directional_and_vertical_uses_pass(self):
        src = "\n".join([
            "EdgeInsetsDirectional.fromSTEB(1, 2, 3, 4)",
            "EdgeInsets.only(top: 4, bottom: f(left: 1))",
            "Positioned(top: 0, bottom: 0, child: x)",
            "PositionedDirectional(start: 0, child: x)",
            "AlignmentDirectional.centerStart",
            "Alignment.topCenter",
            "TextAlign.start",
            "BorderRadius.only(bottom: r)",
        ])
        self.assertEqual(cr.hits(src), [])

    def test_gradient_sides_and_marked_sites_pass(self):
        src = "\n".join([
            "LinearGradient(",
            "  begin: Alignment.topLeft,",
            "  end: Alignment.bottomRight,",
            ")",
            "x(align: TextAlign.left), // rtl-ok: drawn into an image",
            "// rtl-ok: picture coordinates",
            "Positioned(left: 1, child: y)",
        ])
        self.assertEqual(cr.hits(src), [])

    def test_strings_do_not_count(self):
        self.assertEqual(cr.hits("Positioned(child: Text('left: 1'))"), [])


if __name__ == "__main__":
    unittest.main()
