# Copyright (c) 2026 Eric Wijesinghe <EricWijesinghe@hotmail.com>. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Tests for check-notices.py. Run: python3 -I -m unittest discover -s scripts/ci -p 'test_*.py'"""

import importlib.util
import json
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


class DartShipped(unittest.TestCase):
    """pub deps --json shape: an app lists its dev dependencies in both lists; a path package lists its own."""
    DEPS = {"packages": [
        {"name": "customer", "source": "root", "version": "1.0.0", "dependencies": ["flutter", "ui", "lints"],
         "devDependencies": ["lints"]},
        {"name": "partner", "source": "root", "version": "1.0.0", "dependencies": ["flutter"], "devDependencies": []},
        {"name": "console", "source": "root", "version": "1.0.0", "dependencies": [], "devDependencies": []},
        {"name": "ui", "source": "root", "version": "0.1.0", "dependencies": ["http", "mockito"],
         "devDependencies": ["mockito"]},
        {"name": "http", "source": "hosted", "version": "1.6.0", "dependencies": ["meta"], "devDependencies": []},
        {"name": "meta", "source": "hosted", "version": "1.19.0", "dependencies": [], "devDependencies": []},
        {"name": "mockito", "source": "hosted", "version": "5.0.0", "dependencies": [], "devDependencies": []},
        {"name": "lints", "source": "hosted", "version": "6.1.0", "dependencies": [], "devDependencies": []},
        {"name": "flutter", "source": "sdk", "version": "0.0.0", "dependencies": [], "devDependencies": []},
    ]}

    def test_reachable_hosted_packages_through_path_packages(self):
        got = cn.dart_shipped(json.dumps(self.DEPS))
        self.assertEqual(got, {("http", "1.6.0"), ("meta", "1.19.0")})

    def test_dev_only_packages_are_left_out(self):
        names = {name for name, _ in cn.dart_shipped(json.dumps(self.DEPS))}
        self.assertNotIn("mockito", names)
        self.assertNotIn("lints", names)

    def test_missing_app_is_refused(self):
        deps = {"packages": [p for p in self.DEPS["packages"] if p["name"] != "console"]}
        with self.assertRaises(ValueError):
            cn.dart_shipped(json.dumps(deps))


if __name__ == "__main__":
    unittest.main()
