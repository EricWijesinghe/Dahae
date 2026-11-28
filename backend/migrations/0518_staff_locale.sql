-- A staff member's own console language (R4 language picker). NULL follows the install's `install.console_locale`.
-- Customers and pros already carry `users.locale` and `pros.locale`; team members follow their team owner.
-- Additive: the previous release never reads or writes the column. staff is a small table.
ALTER TABLE staff ADD COLUMN locale TEXT CHECK (locale IS NULL OR locale ~ '^[a-z]{2}(-[A-Z]{2})?$');
