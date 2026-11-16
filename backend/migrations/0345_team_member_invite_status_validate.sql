-- Checks the existing team member rows against the invite status rule. Takes only a SHARE UPDATE EXCLUSIVE lock.
ALTER TABLE team_members VALIDATE CONSTRAINT team_members_invite_status_check;
