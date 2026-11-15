-- Groups for the counts services added by the redesign. Migration 0214 grouped only the rows that existed then,
-- and a database that already held whole_home, window or disinfection without a group keeps them ungrouped
-- unless this runs. It only fills rows that have no group when it runs; it does not change rows that already have one.
UPDATE services SET group_code = v.grp
FROM (VALUES ('whole_home', 'living'), ('window', 'living'), ('disinfection', 'disinfection')) AS v (code, grp)
WHERE services.code = v.code AND services.group_code IS NULL;
