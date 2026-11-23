# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Tests for check-migrations.py. Run: python3 -I -m unittest discover -s scripts/ci -p 'test_*.py'"""

import importlib.util
import pathlib
import unittest

_spec = importlib.util.spec_from_file_location("check_migrations", pathlib.Path(__file__).with_name("check-migrations.py"))
cm = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cm)


class Duplicates(unittest.TestCase):
    def test_unique_numbers_pass(self):
        self.assertEqual(cm.check_duplicates(["0001_a.sql", "0002_b.sql"]), [])

    def test_same_number_twice_fails(self):
        errors = cm.check_duplicates(["0040_a.sql", "0040_b.sql"])
        self.assertEqual(len(errors), 1)
        self.assertIn("0040", errors[0])

    def test_bad_name_fails(self):
        self.assertEqual(len(cm.check_duplicates(["19_x.sql"])), 1)


class Released(unittest.TestCase):
    def test_unchanged_passes(self):
        self.assertEqual(cm.check_released({"0001_a.sql": b"x"}, {"0001_a.sql": b"x", "0002_b.sql": b"y"}), [])

    def test_edit_fails(self):
        errors = cm.check_released({"0001_a.sql": b"x"}, {"0001_a.sql": b"x changed"})
        self.assertIn("edited", errors[0])

    def test_delete_fails(self):
        self.assertIn("deleted", cm.check_released({"0001_a.sql": b"x"}, {})[0])

    def test_crlf_checkout_is_not_an_edit(self):
        self.assertEqual(cm.check_released({"0001_a.sql": b"a\nb\n"}, {"0001_a.sql": b"a\r\nb\r\n"}), [])


class NewSql(unittest.TestCase):
    def test_concurrent_index_with_marker_passes(self):
        sql = "-- no-transaction\nCREATE INDEX CONCURRENTLY IF NOT EXISTS i ON bookings (pro_id);\n"
        self.assertEqual(cm.check_new_sql("0041_i.sql", sql), [])

    def test_concurrent_index_without_marker_fails(self):
        sql = "CREATE INDEX CONCURRENTLY i ON bookings (pro_id);\n"
        self.assertTrue(any("no-transaction" in e for e in cm.check_new_sql("0041_i.sql", sql)))

    def test_concurrent_file_with_two_statements_fails(self):
        sql = "-- no-transaction\nCREATE INDEX CONCURRENTLY a ON bookings (pro_id);\nCREATE INDEX CONCURRENTLY b ON bookings (user_id);\n"
        self.assertTrue(any("exactly one" in e for e in cm.check_new_sql("0041_i.sql", sql)))

    def test_plain_index_on_big_table_fails(self):
        errors = cm.check_new_sql("0041_i.sql", "CREATE INDEX i ON bookings (pro_id);")
        self.assertTrue(any("CONCURRENTLY" in e for e in errors))

    def test_plain_index_on_small_table_passes(self):
        self.assertEqual(cm.check_new_sql("0041_i.sql", "CREATE INDEX i ON service_extras (x);"), [])

    def test_index_on_table_created_in_same_file_passes(self):
        sql = "CREATE TABLE widgets (id int);\nCREATE INDEX w ON widgets (id);"
        self.assertEqual(cm.check_new_sql("0041_w.sql", sql), [])

    def test_constraint_without_not_valid_on_big_table_fails(self):
        sql = "ALTER TABLE bookings ADD CONSTRAINT c CHECK (estimate >= 0);"
        self.assertTrue(any("NOT VALID" in e for e in cm.check_new_sql("0040_c.sql", sql)))

    def test_constraint_with_not_valid_passes(self):
        sql = "ALTER TABLE bookings ADD CONSTRAINT c CHECK (estimate >= 0) NOT VALID;"
        self.assertEqual(cm.check_new_sql("0040_c.sql", sql), [])

    def test_comments_are_ignored(self):
        sql = "-- CREATE INDEX i ON bookings (x);\nSELECT 1;"
        self.assertEqual(cm.check_new_sql("0040_x.sql", sql), [])


class Contract(unittest.TestCase):
    RELEASED = (1, 6, 4)

    def check(self, sql):
        return cm.check_new_sql("0400_x.sql", sql, self.RELEASED)

    def test_bare_drop_table_fails(self):
        self.assertTrue(any("contract" in e for e in self.check("DROP TABLE staff_password_resets;")))

    def test_each_destructive_form_fails(self):
        for sql in (
            "ALTER TABLE bookings DROP COLUMN note;",
            "DROP TYPE booking_state;",
            "DROP FUNCTION IF EXISTS f(int);",
            "DROP TRIGGER t ON bookings;",
            "ALTER TABLE bookings DROP CONSTRAINT c;",
            "ALTER TABLE bookings RENAME COLUMN a TO b;",
            "ALTER TABLE bookings ALTER COLUMN a TYPE bigint;",
            "ALTER TABLE bookings ALTER COLUMN a SET DATA TYPE bigint;",
            "ALTER TABLE bookings ALTER COLUMN a SET NOT NULL;",
        ):
            self.assertEqual(len(self.check(sql)), 1, sql)

    def test_marker_at_or_below_the_last_release_passes(self):
        for version in ("v1.6.4", "v1.5.0"):
            sql = f"-- contract: unused since {version}\nDROP TABLE staff_password_resets;"
            self.assertEqual(self.check(sql), [], version)

    def test_marker_above_the_last_release_fails(self):
        sql = "-- contract: unused since v1.6.5\nDROP TABLE staff_password_resets;"
        self.assertEqual(len(self.check(sql)), 1)

    def test_marker_without_a_known_release_fails(self):
        sql = "-- contract: unused since v1.0.0\nDROP TABLE staff_password_resets;"
        self.assertEqual(len(cm.check_new_sql("0400_x.sql", sql)), 1)

    def test_objects_created_in_the_same_file_are_exempt(self):
        sql = (
            "CREATE TABLE tmp_x (a int);\nALTER TABLE tmp_x ALTER COLUMN a SET NOT NULL;\nDROP TABLE tmp_x;\n"
            "DROP TRIGGER IF EXISTS t ON bookings;\nCREATE TRIGGER t AFTER INSERT ON bookings FOR EACH ROW EXECUTE FUNCTION f();\n"
            "DROP FUNCTION IF EXISTS g();\nCREATE OR REPLACE FUNCTION g() RETURNS int AS 'select 1' LANGUAGE sql;"
        )
        self.assertEqual(self.check(sql), [])

    def test_constraint_added_again_is_exempt(self):
        sql = "ALTER TABLE bookings DROP CONSTRAINT IF EXISTS c;\nALTER TABLE bookings ADD CONSTRAINT c CHECK (x > 0) NOT VALID;"
        self.assertEqual(self.check(sql), [])

    def test_column_added_then_set_not_null_fails(self):
        sql = "ALTER TABLE refund_queue ADD COLUMN c bigint;\nALTER TABLE refund_queue ALTER COLUMN c SET NOT NULL;"
        self.assertEqual(len(self.check(sql)), 1)

    def test_names_that_only_contain_the_words_pass(self):
        sql = "ALTER TABLE bookings ADD COLUMN renamed_at timestamptz;\n-- DROP TABLE bookings;\nSELECT 1;"
        self.assertEqual(self.check(sql), [])


if __name__ == "__main__":
    unittest.main()
