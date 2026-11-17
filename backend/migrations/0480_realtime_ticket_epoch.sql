-- Staff session epoch of the access token a socket ticket was issued with (realtime_ticket.rs): a ticket from a
-- session that has since ended no longer opens a socket. Tickets live 30 seconds; rows from before this column
-- read NULL and a staff ticket without an epoch is refused (the app fetches a new one).
ALTER TABLE realtime_tickets ADD COLUMN epoch INT;
