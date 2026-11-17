# Copyright (c) 2026 Eric Wijesinghe <EricWijesinghe@hotmail.com>. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Tests for check-notices.py. Run: python3 -I -m unittest discover -s scripts/ci -p 'test_*.py'"""

import importlib.util
import pathlib
import unittest

_spec = importlib.util.spec_from_file_location("check_notices", pathlib.Path(__file__).with_name("check-notices.py"))
cn = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cn)


class TreeLines(unittest.TestCase):
    TREE = [
        "api v1.5.0 (E:/wt-fd/backend/crates/api)",
        "serde v1.0.0",
        "serde v1.0.0 (*)",
        "sqlx-macros v0.8.6 (proc-macro)",
        "chacha20 v0.10.2",
        "domain v1.5.0 (E:/wt-fd/backend/crates/domain)",
    ]

    def test_registry_and_proc_macro_crates_are_read(self):
        got = cn.tree_crates(self.TREE)
        self.assertEqual(got, {("serde", "1.0.0"), ("sqlx-macros", "0.8.6"), ("chacha20", "0.10.2")})

    def test_workspace_path_members_are_skipped(self):
        names = {name for name, _ in cn.tree_crates(self.TREE)}
        self.assertNotIn("api", names)
        self.assertNotIn("domain", names)

    def test_git_source_is_refused(self):
        with self.assertRaises(ValueError):
            cn.tree_crates(["fork v1.0.0 (https://github.com/x/fork#abc)"])


class Listed(unittest.TestCase):
    def test_reads_name_and_version_from_table_rows(self):
        text = "# x\n| Crate | Version | License | Source |\n|---|---|---|---|\n| chacha20 | 0.10.2 | MIT OR Apache-2.0 | u |\n"
        self.assertIn(("chacha20", "0.10.2"), cn.listed(text))
        self.assertNotIn(("chacha20", "0.9.0"), cn.listed(text))

    def test_prose_lines_are_ignored(self):
        self.assertEqual(cn.listed("chacha20 0.10.2 is used\n"), set())


class PubHosted(unittest.TestCase):
    LOCK = (
        "packages:\n"
        "  args:\n"
        "    dependency: \"direct main\"\n"
        "    description:\n"
        "      name: args\n"
        "      url: \"https://pub.dev\"\n"
        "    source: hosted\n"
        "    version: \"2.7.0\"\n"
        "  flutter:\n"
        "    dependency: \"direct main\"\n"
        "    description: flutter\n"
        "    source: sdk\n"
        "    version: \"0.0.0\"\n"
    )

    def test_hosted_packages_only(self):
        self.assertEqual(cn.pub_hosted(self.LOCK), {("args", "2.7.0")})


if __name__ == "__main__":
    unittest.main()
