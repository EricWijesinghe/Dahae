-- Team members wait for the platform owner's approval before they see addresses or get jobs.
-- Existing invites that were never signed in to are queued for approval; members already active stay active.
-- The old CHECK only allows 'invited', so it goes first; otherwise the UPDATE below violates it.
ALTER TABLE team_members DROP CONSTRAINT IF EXISTS team_members_invite_status_check;
UPDATE team_members SET invite_status = 'pending_approval' WHERE invite_status = 'invited';
ALTER TABLE team_members ALTER COLUMN invite_status SET DEFAULT 'pending_approval';
ALTER TABLE team_members
    ADD CONSTRAINT team_members_invite_status_check CHECK (invite_status IN ('pending_approval', 'active', 'removed'));
