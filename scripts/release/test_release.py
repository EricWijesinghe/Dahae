# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Tests for the release checks: store listing lengths and workflow pins.

  python -I -m unittest scripts/release/test_release.py
"""

import importlib.util
import pathlib
import re
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
TESTDATA = HERE / "testdata"


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


LISTINGS = _load("check_listings", ROOT / "docs" / "store" / "check_listings.py")
WORKFLOWS = _load("check_workflows", HERE / "check_workflows.py")
NOTES = _load("changelog_notes", HERE / "changelog-notes.py")


class ListingLengthTests(unittest.TestCase):
    def test_overlong_fixture_fails_on_app_name(self):
        problems = LISTINGS.check_file(TESTDATA / "listing-overlong.md")
        self.assertTrue(any("row 1" in p and "limit 30" in p for p in problems), problems)

    def test_real_listings_pass(self):
        for path in sorted((ROOT / "docs" / "store").glob("listing-*.md")):
            with self.subTest(file=path.name):
                self.assertEqual(LISTINGS.check_file(path), [])


class ChangelogNotesTests(unittest.TestCase):
    CHANGELOG = (
        "# Changelog\n\n## [1.5.0] - 2026-11-17\n\n### Added\n- New thing.\n\n"
        "## [1.4.0] - 2026-11-13\n\n### Fixed\n- Old thing.\n"
    )

    def test_section_for_tag_without_v(self):
        self.assertEqual(NOTES.section(self.CHANGELOG, "v1.5.0"), "### Added\n- New thing.\n")

    def test_section_stops_at_next_version(self):
        self.assertNotIn("Old thing", NOTES.section(self.CHANGELOG, "v1.5.0"))

    def test_prerelease_section(self):
        text = self.CHANGELOG.replace("## [1.5.0]", "## [1.5.0-beta.1]")
        self.assertEqual(NOTES.section(text, "v1.5.0-beta.1"), "### Added\n- New thing.\n")

    def test_missing_section_fails(self):
        with self.assertRaises(ValueError):
            NOTES.section(self.CHANGELOG, "v9.9.9")

    def test_empty_section_fails(self):
        with self.assertRaises(ValueError):
            NOTES.section("## [1.5.0] - 2026-11-17\n\n## [1.4.0]\n- x\n", "v1.5.0")

    def test_real_changelog_has_the_cargo_version(self):
        text = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
        ver = re.search(r'^version = "([^"]+)"', (ROOT / "backend" / "Cargo.toml").read_text(encoding="utf-8"), re.M).group(1)
        self.assertIn("### Added", NOTES.section(text, "v" + ver))


class WorkflowRuleTests(unittest.TestCase):
    def test_unpinned_action_fails(self):
        found = WORKFLOWS.problems(TESTDATA / "workflow-unpinned.yml")
        self.assertTrue(any("actions/checkout@v4" in p for p in found), found)

    def test_publishing_job_without_environment_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "release.yml"
            path.write_text((TESTDATA / "workflow-unpinned.yml").read_text(encoding="utf-8"), encoding="utf-8")
            found = WORKFLOWS.problems(path)
        self.assertTrue(any("'images' declares no environment" in p for p in found), found)

    def test_release_workflow_passes(self):
        self.assertEqual(WORKFLOWS.problems(ROOT / ".github" / "workflows" / "release.yml"), [])

    def test_every_workflow_passes(self):
        for path in sorted((ROOT / ".github" / "workflows").glob("*.yml")):
            with self.subTest(file=path.name):
                self.assertEqual(WORKFLOWS.problems(path), [])


if __name__ == "__main__":
    unittest.main()
