-- Groups for the counts services added by the redesign. Migration 0214 grouped only the rows that existed then,
-- and a database that already held whole_home, window or disinfection without a group keeps them ungrouped
-- unless this runs. Only rows still without a group are touched, so an owner who removes one stays in control.
UPDATE services SET group_code = v.grp
FROM (VALUES ('whole_home', 'living'), ('window', 'living'), ('disinfection', 'disinfection')) AS v (code, grp)
WHERE services.code = v.code AND services.group_code IS NULL;
